//! `histogram` (the MATLAB histogram object) and `hist` (counts and the
//! classic patch plot).
//!
//! `hist` follows Octave 11.3, measured black-box (`hist` has no MATLAB
//! object of its own; MATLAB also draws a patch): bins are `(a, b]` between
//! the midpoints of the centres, the outer bins are open, NaN is ignored,
//! ±Inf fall in the outer bins, constant data gets unit bins around the
//! value. The plot is one patch per data column with `FaceColor 'flat'`
//! and the column number as colour data (so the colormap colours them).

use crate::error::{GraphicsError, Result};
use crate::graphics::{Graphics, Opts};
use crate::model::{Group, GroupKind, Handle, Histogram, Kind, Paint, Patch};
use crate::value::Array;

/// The bins of `hist`: a bin count or the bin centres.
#[derive(Debug, Clone, PartialEq)]
pub enum HistBins {
    Count(usize),
    Centers(Vec<f64>),
}

/// The result of `hist`: counts (bins × columns) and the centres.
#[derive(Debug, Clone, PartialEq)]
pub struct HistCounts {
    pub counts: Array,
    pub centers: Vec<f64>,
}

/// `[n, x] = hist(y, bins, norm)`. A vector `y` is one column. `norm`
/// scales each column so it sums to `norm`.
pub fn hist_counts(y: &Array, bins: &HistBins, norm: Option<f64>) -> Result<HistCounts> {
    let columns: Vec<&[f64]> = if y.is_vector() || y.is_empty() {
        vec![&y.data[..]]
    } else {
        (0..y.cols()).map(|c| y.column(c)).collect()
    };
    let finite: Vec<f64> = y.data.iter().copied().filter(|v| v.is_finite()).collect();
    let centers = match bins {
        HistBins::Count(0) => return Err(GraphicsError::args("hist", "N must be positive")),
        HistBins::Count(n) => {
            let n = *n;
            let lo = finite.iter().copied().fold(f64::INFINITY, f64::min);
            let hi = finite.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let (lo, hi) = if finite.is_empty() {
                (0.0, 1.0)
            } else {
                (lo, hi)
            };
            if lo == hi {
                // Measured: unit bins with the value in bin floor((n-1)/2)+1.
                let start = lo - ((n - 1) / 2) as f64;
                (0..n).map(|k| start + k as f64).collect()
            } else {
                let delta = (hi - lo) / n as f64 / 2.0;
                linspace(lo + delta, hi - delta, n)
            }
        }
        HistBins::Centers(c) => {
            if c.is_empty() || c.iter().any(|v| !v.is_finite()) {
                return Err(GraphicsError::args("hist", "bin centres must be finite"));
            }
            let mut c = c.clone();
            c.sort_by(f64::total_cmp);
            c
        }
    };
    let nb = centers.len();
    let bounds: Vec<f64> = centers.windows(2).map(|w| (w[0] + w[1]) / 2.0).collect();
    let mut counts = vec![0.0; nb * columns.len()];
    for (j, col) in columns.iter().enumerate() {
        let out = &mut counts[j * nb..(j + 1) * nb];
        for &v in col.iter() {
            if v.is_nan() {
                continue;
            }
            // First bin whose upper bound is >= v: bins are (a, b].
            let k = bounds.partition_point(|b| *b < v);
            out[k] += 1.0;
        }
        if let Some(norm) = norm {
            let total: f64 = out.iter().sum();
            if total > 0.0 {
                for c in out.iter_mut() {
                    *c *= norm / total;
                }
            }
        }
    }
    Ok(HistCounts {
        counts: Array::matrix(nb, columns.len(), counts),
        centers,
    })
}

fn linspace(a: f64, b: f64, n: usize) -> Vec<f64> {
    if n == 1 {
        return vec![b];
    }
    (0..n)
        .map(|k| {
            if k == n - 1 {
                b
            } else {
                a + (b - a) * k as f64 / (n - 1) as f64
            }
        })
        .collect()
}

impl Graphics {
    /// `histogram(X, ...)` with the bin `edges` already chosen (the
    /// interpreter takes them from `histcounts`, so `Values` always equals
    /// `histcounts` for the same arguments). `bin_method` is `auto` when
    /// the edges are automatic, otherwise `manual`.
    pub fn histogram(
        &mut self,
        ax: Option<Handle>,
        data: Vec<f64>,
        edges: Vec<f64>,
        bin_method: &str,
        opts: &Opts,
    ) -> Result<Handle> {
        if edges.len() < 2
            || edges.iter().any(|e| e.is_nan())
            || edges.windows(2).any(|w| w[0] > w[1])
        {
            return Err(GraphicsError::args(
                "histogram",
                "bin edges must be a nondecreasing vector of two or more values",
            ));
        }
        let ax = self.new_plot(ax)?;
        let color = self.axes_mut(ax)?.next_color();
        let mut h = Histogram::new(data, edges, color);
        h.bin_method = bin_method.to_string();
        let h = self.add(
            ax,
            Kind::Group(Box::new(Group {
                kind: GroupKind::Histogram(Box::new(h)),
                display_name: String::new(),
            })),
        );
        self.apply_opts(&[h], opts)?;
        Ok(h)
    }

    /// The plot of `hist`: one patch per column of `counts` (bins ×
    /// columns). One column fills each bin; several columns share 0.64 of
    /// the bin width side by side (measured).
    pub fn hist_plot(
        &mut self,
        ax: Option<Handle>,
        result: &HistCounts,
        opts: &Opts,
    ) -> Result<Vec<Handle>> {
        let c = &result.centers;
        let nb = c.len();
        if nb == 0 || result.counts.rows() != nb {
            return Err(GraphicsError::args("hist", "counts do not match the bins"));
        }
        let m = result.counts.cols().max(1);
        let ax = self.new_plot(ax)?;
        let fresh = !self.axes_ref(ax)?.hold_add;
        // Measured: integer centres become manual x ticks (as for bar).
        if fresh && c.iter().all(|v| v.fract() == 0.0) {
            let a = self.axes_mut(ax)?;
            a.x.tick = c.clone();
            a.x.tick_mode = crate::model::Mode::Manual;
        }
        let d = if nb > 1 { c[1] - c[0] } else { 1.0 };
        let mut out = Vec::with_capacity(m);
        for j in 0..m {
            let counts = result.counts.column(j);
            let mut vx = Vec::with_capacity(4 * nb);
            let mut vy = Vec::with_capacity(4 * nb);
            for k in 0..nb {
                let (l, r) = if m == 1 {
                    let left = if k == 0 {
                        c[0] - d / 2.0
                    } else {
                        (c[k - 1] + c[k]) / 2.0
                    };
                    let right = if k + 1 == nb {
                        c[k] + d / 2.0
                    } else {
                        (c[k] + c[k + 1]) / 2.0
                    };
                    (left, right)
                } else {
                    let w = 0.64 * d / m as f64;
                    let l = c[k] - 0.32 * d + j as f64 * w;
                    (l, l + w)
                };
                for (px, py) in [(l, 0.0), (l, counts[k]), (r, counts[k]), (r, 0.0)] {
                    vx.push(px);
                    vy.push(py);
                }
            }
            let mut v = vx;
            v.extend(vy);
            let mut faces = vec![0.0; 4 * nb];
            for k in 0..nb {
                for i in 0..4 {
                    faces[k + i * nb] = (4 * k + i + 1) as f64;
                }
            }
            let p = Patch {
                vertices: Array::matrix(4 * nb, 2, v),
                faces: Array::matrix(nb, 4, faces),
                fvcdata: Array::scalar((j + 1) as f64),
                face_color: Paint::Flat,
                edge_color: Paint::Rgb([0.0, 0.0, 0.0]),
                cdata_direct: false,
                line_style: "-".into(),
                line_width: 0.5,
                marker: "none".into(),
                face_alpha: 1.0,
                edge_alpha: 1.0,
                display_name: String::new(),
            };
            out.push(self.add(ax, Kind::Patch(p)));
        }
        self.apply_opts(&out, opts)?;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counts(y: &[f64], bins: HistBins) -> (Vec<f64>, Vec<f64>) {
        let r = hist_counts(&Array::row(y), &bins, None).unwrap();
        (r.counts.data, r.centers)
    }

    #[test]
    fn hist_matches_octave() {
        let y = [1.0, 2.0, 2.0, 3.0, 3.0, 3.0, 4.0, 7.0, 7.5, 10.0];
        let (n, c) = counts(&y, HistBins::Count(10));
        assert_eq!(n, [1., 2., 3., 1., 0., 0., 1., 1., 0., 1.]);
        assert!((c[0] - 1.45).abs() < 1e-12 && (c[9] - 9.55).abs() < 1e-12);
        assert_eq!(counts(&y, HistBins::Count(3)).0, [7., 1., 2.]);
        let (n, c) = counts(&y, HistBins::Centers(vec![0., 5., 10.]));
        assert_eq!((n, c), (vec![3., 6., 1.], vec![0., 5., 10.]));
        assert_eq!(
            counts(&[0., 1., 2., 3., 4.], HistBins::Count(2)).0,
            [3., 2.]
        );
        let (n, c) = counts(&[5., 5., 5.], HistBins::Count(3));
        assert_eq!((n, c), (vec![0., 3., 0.], vec![4., 5., 6.]));
        assert_eq!(counts(&[0., 0.], HistBins::Count(4)).1, [-1., 0., 1., 2.]);
        let (n, _) = counts(
            &[1., f64::NAN, 3., f64::INFINITY, f64::NEG_INFINITY],
            HistBins::Count(10),
        );
        assert_eq!(n, [2., 0., 0., 0., 0., 0., 0., 0., 0., 2.]);
        let r = hist_counts(
            &Array::row(&[1., 2., 3., 4., 5., 6.]),
            &HistBins::Count(3),
            Some(12.0),
        )
        .unwrap();
        assert_eq!(r.counts.data, [4., 4., 4.]);
        let r = hist_counts(
            &Array::matrix(3, 2, vec![1., 3., 5., 2., 4., 5.]),
            &HistBins::Count(3),
            None,
        )
        .unwrap();
        assert_eq!(r.counts.data, [1., 1., 1., 1., 0., 2.]);
    }

    #[test]
    fn histogram_values_follow_normalization() {
        let mut h = Histogram::new(vec![0.5, 1.5, 1.5, 2.0], vec![0., 1., 2.], [0.0; 3]);
        assert_eq!(h.counts, [1., 3.]);
        for (norm, want) in [
            ("count", vec![1., 3.]),
            ("probability", vec![0.25, 0.75]),
            ("pdf", vec![0.25, 0.75]),
            ("countdensity", vec![1., 3.]),
            ("cumcount", vec![1., 4.]),
            ("cdf", vec![0.25, 1.0]),
        ] {
            h.normalization = norm.into();
            assert_eq!(h.values(), want, "{norm}");
        }
    }
}
