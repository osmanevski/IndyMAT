//! Splitting raw MATLAB argument lists for the interpreter hook, plus
//! change tracking for figure export.
//!
//! `plot(x, y, 'r--', x2, y2, 'LineWidth', 2)` arrives as a list of values;
//! [`split_series`] turns it into [`PlotSeries`] groups and name/value
//! options with MATLAB's rule: after the data groups, the first string that
//! is not a LineSpec starts the name/value pairs.

use crate::error::{GraphicsError, Result};
use crate::graphics::Graphics;
use crate::linespec;
use crate::model::Handle;
use crate::plots::PlotSeries;
use crate::value::PropValue;

/// Data groups and trailing name/value options of a plotting call.
pub type SplitArgs = (Vec<PlotSeries>, Vec<(String, PropValue)>);

/// Splits `plot`/`plot3`/`stairs`/`stem` arguments. `dims` is 2 (x, y) or
/// 3 (x, y, z); a lone numeric argument is Y with implicit X (2-D only).
pub fn split_series(func: &str, args: &[PropValue], dims: usize) -> Result<SplitArgs> {
    let mut series = Vec::new();
    let mut numeric = Vec::new();
    let mut i = 0;
    let flush = |numeric: &mut Vec<crate::value::Array>,
                 spec: Option<String>,
                 series: &mut Vec<PlotSeries>|
     -> Result<()> {
        let group = std::mem::take(numeric);
        match (dims, group.len()) {
            (_, 0) if spec.is_none() => {}
            (2, 1) => series.push(PlotSeries {
                x: None,
                y: group[0].clone(),
                z: None,
                spec,
            }),
            (2, n) if n % 2 == 0 => {
                let last = n / 2 - 1;
                for (k, pair) in group.chunks(2).enumerate() {
                    series.push(PlotSeries {
                        x: Some(pair[0].clone()),
                        y: pair[1].clone(),
                        z: None,
                        spec: if k == last { spec.clone() } else { None },
                    });
                }
            }
            (3, n) if n % 3 == 0 && n > 0 => {
                let last = n / 3 - 1;
                for (k, t) in group.chunks(3).enumerate() {
                    series.push(PlotSeries {
                        x: Some(t[0].clone()),
                        y: t[1].clone(),
                        z: Some(t[2].clone()),
                        spec: if k == last { spec.clone() } else { None },
                    });
                }
            }
            _ => {
                return Err(GraphicsError::args(
                    func,
                    "data arguments must come in X, Y[, Z] groups",
                ));
            }
        }
        Ok(())
    };
    while i < args.len() {
        match &args[i] {
            PropValue::Str(s) => {
                if linespec::parse(s)?.is_some() && !numeric.is_empty() {
                    flush(&mut numeric, Some(s.clone()), &mut series)?;
                    i += 1;
                } else {
                    break;
                }
            }
            v => {
                let a = v
                    .as_array()
                    .ok_or_else(|| GraphicsError::args(func, "invalid data argument"))?;
                numeric.push(a);
                i += 1;
            }
        }
    }
    flush(&mut numeric, None, &mut series)?;
    if series.is_empty() {
        return Err(GraphicsError::args(func, "not enough input arguments"));
    }
    let opts = name_value(func, &args[i..])?;
    Ok((series, opts))
}

/// Pairs trailing `'Name', value` arguments.
pub fn name_value(func: &str, args: &[PropValue]) -> Result<Vec<(String, PropValue)>> {
    if !args.len().is_multiple_of(2) {
        return Err(GraphicsError::args(
            func,
            "property names and values must come in pairs",
        ));
    }
    args.chunks(2)
        .map(|p| match &p[0] {
            PropValue::Str(name) => Ok((name.clone(), p[1].clone())),
            _ => Err(GraphicsError::args(func, "property name must be a string")),
        })
        .collect()
}

impl Graphics {
    /// Figures whose Figure-Data JSON changed since the previous call, with
    /// that JSON; closed figures are reported with `None`. The kernel calls
    /// this after each statement to know what to (re)export.
    pub fn take_changed(&mut self) -> Vec<(Handle, Option<String>)> {
        let mut out = Vec::new();
        for f in self.figure_handles() {
            let text = self.figure_json(f);
            if self.exported.get(&f) != Some(&text) {
                self.exported.insert(f, text.clone());
                out.push((f, Some(text)));
            }
        }
        let open = self.figure_handles();
        let closed: Vec<Handle> = self
            .exported
            .keys()
            .copied()
            .filter(|f| !open.contains(f))
            .collect();
        for f in closed {
            self.exported.remove(&f);
            out.push((f, None));
        }
        out.sort_by_key(|(h, _)| *h);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::Array;

    fn n(v: &[f64]) -> PropValue {
        PropValue::Array(Array::row(v))
    }

    #[test]
    fn splits_groups_specs_and_options() {
        let args = [
            n(&[1., 2.]),
            n(&[3., 4.]),
            PropValue::str("r--"),
            n(&[5., 6.]),
            PropValue::str("LineWidth"),
            PropValue::Num(2.0),
        ];
        let (s, o) = split_series("plot", &args, 2).unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].spec.as_deref(), Some("r--"));
        assert!(s[1].x.is_none(), "a lone trailing array is Y");
        assert_eq!(o, vec![("LineWidth".to_string(), PropValue::Num(2.0))]);
        assert!(split_series("plot", &[PropValue::str("r")], 2).is_err());
        assert!(split_series("plot", &[n(&[1.]), PropValue::str("Color")], 2).is_err());
        let (s3, _) = split_series(
            "plot3",
            &[n(&[1.]), n(&[2.]), n(&[3.]), PropValue::str("o")],
            3,
        )
        .unwrap();
        assert_eq!(s3[0].spec.as_deref(), Some("o"));
    }

    #[test]
    fn change_tracking() {
        let mut g = Graphics::new();
        g.plot(None, &[PlotSeries::y(&[1., 2.])], &[]).unwrap();
        assert_eq!(g.take_changed().len(), 1);
        assert!(g.take_changed().is_empty());
        g.grid(None, Some(true), false).unwrap();
        assert_eq!(g.take_changed().len(), 1);
        g.close(crate::graphics::CloseTarget::All).unwrap();
        assert_eq!(g.take_changed(), vec![(Handle(1), None)]);
    }
}
