//! `set` / `get` by MATLAB property name (case-insensitive, full names).

use crate::color::{Rgb, named};
use crate::error::{GraphicsError, Result};
use crate::graphics::Graphics;
use crate::linespec::{canonical_line_style, canonical_marker};
use crate::model::{GroupKind, Handle, Kind, Mode, Paint, Scale, ScatterColor};
use crate::value::{Array, PropValue};

/// Parses a colour value: RGB triple, colour letter/name, or one of the
/// `keywords` (`none`, `auto`, `flat`, `interp`) allowed for the property.
pub fn parse_paint(v: &PropValue, name: &str, keywords: &[&str]) -> Result<Paint> {
    match v {
        PropValue::Str(s) => {
            let l = s.to_ascii_lowercase();
            if keywords.contains(&l.as_str()) {
                return Ok(match l.as_str() {
                    "none" => Paint::None,
                    "auto" => Paint::Auto,
                    "flat" => Paint::Flat,
                    _ => Paint::Interp,
                });
            }
            named(&l)
                .map(Paint::Rgb)
                .ok_or_else(|| GraphicsError::bad_value(name))
        }
        other => {
            let n = other
                .numbers()
                .ok_or_else(|| GraphicsError::bad_value(name))?;
            if n.len() == 3 && n.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)) {
                Ok(Paint::Rgb([n[0], n[1], n[2]]))
            } else {
                Err(GraphicsError::bad_value(name))
            }
        }
    }
}

fn rgb_value(v: &PropValue, name: &str) -> Result<Rgb> {
    match parse_paint(v, name, &[])? {
        Paint::Rgb(c) => Ok(c),
        _ => Err(GraphicsError::bad_value(name)),
    }
}

fn paint_value(p: Paint) -> PropValue {
    match p {
        Paint::Rgb(c) => PropValue::row(&c),
        Paint::None => PropValue::str("none"),
        Paint::Auto => PropValue::str("auto"),
        Paint::Flat => PropValue::str("flat"),
        Paint::Interp => PropValue::str("interp"),
    }
}

fn scalar(v: &PropValue, name: &str) -> Result<f64> {
    match v.numbers() {
        Some(n) if n.len() == 1 && n[0].is_finite() => Ok(n[0]),
        _ => Err(GraphicsError::bad_value(name)),
    }
}

fn positive(v: &PropValue, name: &str) -> Result<f64> {
    let x = scalar(v, name)?;
    if x > 0.0 {
        Ok(x)
    } else {
        Err(GraphicsError::bad_value(name))
    }
}

fn vector(v: &PropValue, name: &str) -> Result<Vec<f64>> {
    v.numbers().ok_or_else(|| GraphicsError::bad_value(name))
}

fn fixed<const N: usize>(v: &PropValue, name: &str) -> Result<[f64; N]> {
    let n = vector(v, name)?;
    if n.len() != N || n.iter().any(|x| !x.is_finite()) {
        return Err(GraphicsError::bad_value(name));
    }
    let mut out = [0.0; N];
    out.copy_from_slice(&n);
    Ok(out)
}

fn on_off(v: &PropValue, name: &str) -> Result<bool> {
    match v {
        PropValue::Bool(b) => Ok(*b),
        PropValue::Str(s) => match s.to_ascii_lowercase().as_str() {
            "on" => Ok(true),
            "off" => Ok(false),
            _ => Err(GraphicsError::bad_value(name)),
        },
        PropValue::Num(n) if *n == 0.0 || *n == 1.0 => Ok(*n == 1.0),
        _ => Err(GraphicsError::bad_value(name)),
    }
}

fn string(v: &PropValue, name: &str) -> Result<String> {
    match v {
        PropValue::Str(s) => Ok(s.clone()),
        PropValue::Cell(c) => Ok(c.join("\n")),
        PropValue::Num(n) => Ok(crate::layout::format_g(*n, 5)),
        _ => Err(GraphicsError::bad_value(name)),
    }
}

fn strings(v: &PropValue, name: &str) -> Result<Vec<String>> {
    match v {
        PropValue::Str(s) => Ok(if s.is_empty() {
            Vec::new()
        } else {
            s.split('|').map(str::to_string).collect()
        }),
        PropValue::Cell(c) => Ok(c.clone()),
        PropValue::Num(n) => Ok(vec![crate::layout::format_g(*n, 5)]),
        PropValue::Array(a) => Ok(a
            .data
            .iter()
            .map(|v| crate::layout::format_g(*v, 5))
            .collect()),
        _ => Err(GraphicsError::bad_value(name)),
    }
}

fn text_lines(v: &PropValue, name: &str) -> Result<Vec<String>> {
    match v {
        PropValue::Str(s) => Ok(vec![s.clone()]),
        PropValue::Cell(c) => Ok(c.clone()),
        PropValue::Num(n) => Ok(vec![crate::layout::format_g(*n, 5)]),
        _ => Err(GraphicsError::bad_value(name)),
    }
}

fn mode(v: &PropValue, name: &str) -> Result<Mode> {
    match v.as_str().map(str::to_ascii_lowercase).as_deref() {
        Some("auto") => Ok(Mode::Auto),
        Some("manual") => Ok(Mode::Manual),
        _ => Err(GraphicsError::bad_value(name)),
    }
}

fn line_style(v: &PropValue, name: &str) -> Result<String> {
    v.as_str()
        .and_then(canonical_line_style)
        .map(str::to_string)
        .ok_or_else(|| GraphicsError::bad_value(name))
}

fn marker(v: &PropValue, name: &str) -> Result<String> {
    v.as_str()
        .and_then(canonical_marker)
        .map(str::to_string)
        .ok_or_else(|| GraphicsError::bad_value(name))
}

fn choice(v: &PropValue, name: &str, options: &[&str]) -> Result<String> {
    let s = v
        .as_str()
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| GraphicsError::bad_value(name))?;
    if options.contains(&s.as_str()) {
        Ok(s)
    } else {
        Err(GraphicsError::bad_value(name))
    }
}

/// `Units`: only the unit positions are kept in is accepted; converting
/// positions to other units is not implemented (explicit error).
fn only_units(v: &PropValue, name: &str, unit: &str) -> Result<()> {
    let s = choice(
        v,
        name,
        &[
            "normalized",
            "pixels",
            "points",
            "inches",
            "centimeters",
            "characters",
        ],
    )?;
    if s == unit {
        Ok(())
    } else {
        Err(GraphicsError::unsupported(
            "set",
            format!("{name} '{s}' (positions are kept in {unit})"),
        ))
    }
}

fn array(v: &PropValue, name: &str) -> Result<Array> {
    v.as_array().ok_or_else(|| GraphicsError::bad_value(name))
}

impl Graphics {
    /// `set(h, name, value)`.
    pub fn set(&mut self, h: Handle, name: &str, value: PropValue) -> Result<()> {
        let key = name.to_ascii_lowercase();
        let v = &value;
        // Properties shared by every object.
        match key.as_str() {
            "visible" => {
                self.obj_mut(h)?.visible = on_off(v, name)?;
                return Ok(());
            }
            "tag" => {
                self.obj_mut(h)?.tag = string(v, name)?;
                return Ok(());
            }
            "displayname" if !matches!(self.obj(h)?.kind, Kind::Figure(_) | Kind::Axes(_)) => {
                let s = string(v, name)?;
                self.set_display_name(h, &s);
                return Ok(());
            }
            "type" | "parent" | "children" => {
                return Err(GraphicsError::new(
                    "MATLAB:hg:propswch:PropertyReadOnly",
                    format!("property '{name}' is read-only"),
                ));
            }
            _ => {}
        }
        let kind_name = self.obj(h)?.kind.type_name();
        let bad = || GraphicsError::bad_property(kind_name, name);
        if let Kind::Axes(_) = self.obj(h)?.kind {
            return self.set_axes(h, &key, name, v);
        }
        match &mut self.obj_mut(h)?.kind {
            Kind::Figure(f) => match key.as_str() {
                "name" => f.name = string(v, name)?,
                "color" => f.color = rgb_value(v, name)?,
                "colormap" => f.colormap = colormap_rows(v, name)?,
                "position" => f.position = fixed(v, name)?,
                "units" => only_units(v, name, "pixels")?,
                "defaultaxescolororder" => f.default_color_order = Some(colormap_rows(v, name)?),
                "nextplot" => {
                    f.hold_add = choice(v, name, &["add", "replace", "replacechildren"])? == "add"
                }
                "currentaxes" => {
                    let PropValue::Handle(ax) = v else {
                        return Err(GraphicsError::bad_value(name));
                    };
                    let ax = *ax;
                    return self.make_current(ax);
                }
                _ => return Err(bad()),
            },
            Kind::Line(l) => match key.as_str() {
                "xdata" => l.x = vector(v, name)?,
                "ydata" => l.y = vector(v, name)?,
                "zdata" => l.z = vector(v, name)?,
                "color" => l.color = rgb_value(v, name)?,
                "linestyle" => l.line_style = line_style(v, name)?,
                "linewidth" => l.line_width = positive(v, name)?,
                "marker" => l.marker = marker(v, name)?,
                "markersize" => l.marker_size = positive(v, name)?,
                "markeredgecolor" => l.marker_edge = parse_paint(v, name, &["none", "auto"])?,
                "markerfacecolor" => l.marker_face = parse_paint(v, name, &["none", "auto"])?,
                _ => return Err(bad()),
            },
            Kind::Scatter(s) => match key.as_str() {
                "xdata" => s.x = vector(v, name)?,
                "ydata" => s.y = vector(v, name)?,
                "zdata" => s.z = vector(v, name)?,
                "sizedata" => s.size_data = vector(v, name)?,
                "cdata" => {
                    s.cdata = match parse_paint(v, name, &[]) {
                        Ok(Paint::Rgb(c)) => ScatterColor::Rgb(c),
                        _ => {
                            let a = array(v, name)?;
                            if a.numel() == 1 {
                                ScatterColor::Indexed(a.data[0])
                            } else {
                                ScatterColor::PerPoint(a)
                            }
                        }
                    }
                }
                "marker" => s.marker = marker(v, name)?,
                "markeredgecolor" => {
                    s.marker_edge = parse_paint(v, name, &["none", "flat", "auto"])?
                }
                "markerfacecolor" => {
                    s.marker_face = parse_paint(v, name, &["none", "flat", "auto"])?
                }
                "linewidth" => s.line_width = positive(v, name)?,
                _ => return Err(bad()),
            },
            Kind::Surface(s) => match key.as_str() {
                "xdata" => s.x = array(v, name)?,
                "ydata" => s.y = array(v, name)?,
                "zdata" => s.z = array(v, name)?,
                "cdata" => s.c = array(v, name)?,
                "facecolor" => s.face_color = parse_paint(v, name, &["none", "flat", "interp"])?,
                "edgecolor" => s.edge_color = parse_paint(v, name, &["none", "flat", "interp"])?,
                "meshstyle" => s.mesh_style = choice(v, name, &["both", "row", "column"])?,
                "linestyle" => s.line_style = line_style(v, name)?,
                "linewidth" => s.line_width = positive(v, name)?,
                "cdatamapping" => {
                    s.cdata_direct = choice(v, name, &["scaled", "direct"])? == "direct"
                }
                "facealpha" => s.face_alpha = scalar(v, name)?,
                "edgealpha" => s.edge_alpha = scalar(v, name)?,
                "marker" => s.marker = marker(v, name)?,
                _ => return Err(bad()),
            },
            Kind::Patch(p) => match key.as_str() {
                "vertices" => p.vertices = array(v, name)?,
                "faces" => p.faces = array(v, name)?,
                "facevertexcdata" | "cdata" => p.fvcdata = array(v, name)?,
                "facecolor" => p.face_color = parse_paint(v, name, &["none", "flat", "interp"])?,
                "edgecolor" => p.edge_color = parse_paint(v, name, &["none", "flat", "interp"])?,
                "cdatamapping" => {
                    p.cdata_direct = choice(v, name, &["scaled", "direct"])? == "direct"
                }
                "linestyle" => p.line_style = line_style(v, name)?,
                "linewidth" => p.line_width = positive(v, name)?,
                "marker" => p.marker = marker(v, name)?,
                "facealpha" => p.face_alpha = scalar(v, name)?,
                "edgealpha" => p.edge_alpha = scalar(v, name)?,
                _ => return Err(bad()),
            },
            Kind::Text(t) => match key.as_str() {
                "string" => t.lines = text_lines(v, name)?,
                "position" => {
                    let n = vector(v, name)?;
                    if !(n.len() == 2 || n.len() == 3) {
                        return Err(GraphicsError::bad_value(name));
                    }
                    t.position = [n[0], n[1], n.get(2).copied().unwrap_or(0.0)];
                }
                "units" => {
                    t.units = choice(
                        v,
                        name,
                        &[
                            "data",
                            "normalized",
                            "pixels",
                            "points",
                            "inches",
                            "centimeters",
                            "characters",
                        ],
                    )?
                }
                "fontsize" => t.font_size = positive(v, name)?,
                "fontweight" => {
                    t.font_weight = choice(v, name, &["normal", "bold", "demi", "light"])?
                }
                "fontangle" => t.font_angle = choice(v, name, &["normal", "italic", "oblique"])?,
                "color" => t.color = rgb_value(v, name)?,
                "backgroundcolor" => t.background = parse_paint(v, name, &["none"])?,
                "edgecolor" => t.edge_color = parse_paint(v, name, &["none"])?,
                "margin" => t.margin = scalar(v, name)?,
                "horizontalalignment" => {
                    t.horizontal_alignment = choice(v, name, &["left", "center", "right"])?
                }
                "verticalalignment" => {
                    t.vertical_alignment =
                        choice(v, name, &["top", "cap", "middle", "baseline", "bottom"])?
                }
                "rotation" => t.rotation = scalar(v, name)?,
                "interpreter" => t.interpreter = choice(v, name, &["tex", "none", "latex"])?,
                "linestyle" => t.line_style = line_style(v, name)?,
                "linewidth" => t.line_width = positive(v, name)?,
                "clipping" => t.clipping = on_off(v, name)?,
                _ => return Err(bad()),
            },
            Kind::Image(i) => match key.as_str() {
                "cdata" => i.cdata = array(v, name)?,
                "xdata" => i.x = fixed(v, name)?,
                "ydata" => i.y = fixed(v, name)?,
                "cdatamapping" => {
                    i.cdata_direct = choice(v, name, &["scaled", "direct"])? == "direct"
                }
                _ => return Err(bad()),
            },
            Kind::Group(g) => {
                let t = kind_name;
                set_group(&mut g.kind, &key, name, v).map_err(|e| {
                    if e.id == "MATLAB:hg:InvalidProperty" {
                        GraphicsError::bad_property(t, name)
                    } else {
                        e
                    }
                })?
            }
            Kind::Legend(l) => match key.as_str() {
                "string" => l.labels = strings(v, name)?,
                "location" => {
                    let s = choice(v, name, &crate::annot::LEGEND_LOCATIONS)?;
                    l.location = s;
                }
                _ => return Err(bad()),
            },
            Kind::Colorbar(c) => match key.as_str() {
                "label" => c.label = string(v, name)?,
                "location" => {
                    let s = v
                        .as_str()
                        .map(str::to_ascii_lowercase)
                        .ok_or_else(|| GraphicsError::bad_value(name))?;
                    if crate::layout::colorbar_geometry(&s, [0.0, 0.0, 1.0, 1.0]).is_none() {
                        return Err(GraphicsError::bad_value(name));
                    }
                    let peer = c.peer;
                    c.location = s.clone();
                    let base = self.axes_ref(peer)?.base_position;
                    if let Some((p, _, _)) = crate::layout::colorbar_geometry(&s, base) {
                        self.axes_mut(peer)?.position = p;
                    }
                }
                _ => return Err(bad()),
            },
            Kind::Axes(_) => unreachable!("handled above"),
        }
        Ok(())
    }

    fn set_axes(&mut self, h: Handle, key: &str, name: &str, v: &PropValue) -> Result<()> {
        let axis_index = |c: u8| match c {
            b'x' => Some(0),
            b'y' => Some(1),
            b'z' => Some(2),
            _ => None,
        };
        if let Some(i) = axis_index(key.as_bytes()[0]) {
            let rest = &key[1..];
            let matched = matches!(
                rest,
                "lim"
                    | "limmode"
                    | "tick"
                    | "tickmode"
                    | "ticklabel"
                    | "ticklabelmode"
                    | "scale"
                    | "dir"
                    | "grid"
                    | "minorgrid"
            );
            if matched {
                if rest == "lim" {
                    let l = fixed::<2>(v, name)?;
                    return self.set_limits(Some(h), i, Some(l));
                }
                let a = self.axes_mut(h)?;
                let p = a.axis_mut(i);
                match rest {
                    "limmode" => p.lim_mode = mode(v, name)?,
                    "tick" => {
                        let mut t = vector(v, name)?;
                        if t.windows(2).any(|w| w[0] >= w[1]) || t.iter().any(|x| !x.is_finite()) {
                            return Err(GraphicsError::bad_value(name));
                        }
                        t.dedup();
                        p.tick = t;
                        p.tick_mode = Mode::Manual;
                    }
                    "tickmode" => p.tick_mode = mode(v, name)?,
                    "ticklabel" => {
                        p.ticklabel = strings(v, name)?;
                        p.ticklabel_mode = Mode::Manual;
                    }
                    "ticklabelmode" => p.ticklabel_mode = mode(v, name)?,
                    "scale" => {
                        p.scale = if choice(v, name, &["linear", "log"])? == "log" {
                            Scale::Log
                        } else {
                            Scale::Linear
                        }
                    }
                    "dir" => p.reverse = choice(v, name, &["normal", "reverse"])? == "reverse",
                    "grid" => p.grid = on_off(v, name)?,
                    "minorgrid" => p.minor_grid = on_off(v, name)?,
                    _ => unreachable!(),
                }
                return Ok(());
            }
        }
        match key {
            "view" => return self.view(Some(h), fixed(v, name)?),
            "clim" => return self.caxis(Some(h), Some(fixed(v, name)?)),
            "title" | "xlabel" | "ylabel" | "zlabel" => {
                return Err(GraphicsError::new(
                    "MATLAB:hg:InvalidPropertyValue",
                    format!("set the '{name}' text through its handle (get(ax,'{name}'))"),
                ));
            }
            _ => {}
        }
        let a = self.axes_mut(h)?;
        match key {
            "climmode" => a.clim_mode = mode(v, name)?,
            "dataaspectratio" => {
                let d: [f64; 3] = fixed(v, name)?;
                if d.iter().any(|x| *x <= 0.0) {
                    return Err(GraphicsError::bad_value(name));
                }
                a.dar = d;
                a.dar_mode = Mode::Manual;
            }
            "dataaspectratiomode" => a.dar_mode = mode(v, name)?,
            "plotboxaspectratio" => {
                let d: [f64; 3] = fixed(v, name)?;
                if d.iter().any(|x| *x <= 0.0) {
                    return Err(GraphicsError::bad_value(name));
                }
                a.pbar = d;
                a.pbar_mode = Mode::Manual;
            }
            "plotboxaspectratiomode" => a.pbar_mode = mode(v, name)?,
            "colormap" => a.colormap = Some(colormap_rows(v, name)?),
            "colororder" => {
                let arr = array(v, name)?;
                if arr.cols() != 3
                    || arr.rows() == 0
                    || arr.data.iter().any(|x| !(0.0..=1.0).contains(x))
                {
                    return Err(GraphicsError::bad_value(name));
                }
                a.color_order = (0..arr.rows())
                    .map(|r| [arr.at(r, 0), arr.at(r, 1), arr.at(r, 2)])
                    .collect();
                a.color_order_index = 0;
            }
            "colororderindex" => {
                let k = scalar(v, name)?;
                if k < 1.0 || k.fract() != 0.0 {
                    return Err(GraphicsError::bad_value(name));
                }
                a.color_order_index = k as usize - 1;
            }
            "nextplot" => {
                a.hold_add = choice(v, name, &["add", "replace", "replacechildren"])? == "add"
            }
            "position" => {
                let p: [f64; 4] = fixed(v, name)?;
                if p[2] <= 0.0 || p[3] <= 0.0 {
                    return Err(GraphicsError::bad_value(name));
                }
                a.position = p;
                a.base_position = p;
                // A user position ends the sgtitle layout of this axes.
                a.sg_base = None;
            }
            "color" => a.color = parse_paint(v, name, &["none"])?,
            "box" => a.box_on = on_off(v, name)?,
            "yaxislocation" => {
                a.y_right = choice(v, name, &["left", "right", "origin"])? == "right"
            }
            "fontsize" => a.font_size = positive(v, name)?,
            "units" => only_units(v, name, "normalized")?,
            "ticklabelinterpreter" => {
                a.tick_label_interpreter = choice(v, name, &["tex", "none", "latex"])?
            }
            "projection" => {
                if choice(v, name, &["orthographic", "perspective"])? != "orthographic" {
                    return Err(GraphicsError::unsupported("set", "perspective projection"));
                }
            }
            _ => return Err(GraphicsError::bad_property("axes", name)),
        }
        Ok(())
    }

    /// `get(h, name)`.
    pub fn get(&self, h: Handle, name: &str) -> Result<PropValue> {
        let key = name.to_ascii_lowercase();
        if h == Handle::ROOT {
            return match key.as_str() {
                "type" => Ok(PropValue::str("root")),
                "currentfigure" => Ok(match self.current_figure {
                    Some(f) => PropValue::Handle(f),
                    None => PropValue::Handles(Vec::new()),
                }),
                "children" => {
                    let mut v = self.figures.clone();
                    v.reverse();
                    Ok(PropValue::Handles(v))
                }
                "defaultaxescolororder" => Ok(colormap_value(&crate::color::COLOR_ORDER)),
                "defaultaxescolor" => Ok(PropValue::row(&[1.0, 1.0, 1.0])),
                _ => Err(GraphicsError::bad_property("root", name)),
            };
        }
        let o = self.obj(h)?;
        match key.as_str() {
            "type" => return Ok(PropValue::str(o.kind.type_name())),
            "parent" => return Ok(PropValue::Handle(o.parent)),
            "children" => {
                let mut v = o.children.clone();
                v.reverse();
                return Ok(PropValue::Handles(v));
            }
            "visible" => return Ok(PropValue::on_off(o.visible)),
            "tag" => return Ok(PropValue::str(&o.tag)),
            "displayname" if !matches!(o.kind, Kind::Figure(_) | Kind::Axes(_)) => {
                return Ok(PropValue::Str(self.display_name(h)));
            }
            _ => {}
        }
        let bad = || GraphicsError::bad_property(o.kind.type_name(), name);
        Ok(match &o.kind {
            Kind::Figure(f) => match key.as_str() {
                "number" => PropValue::Num(f.number as f64),
                "name" => PropValue::str(&f.name),
                "color" => PropValue::row(&f.color),
                "colormap" => colormap_value(&f.colormap),
                "position" => PropValue::row(&f.position),
                "units" => PropValue::str("pixels"),
                "defaultaxescolororder" => colormap_value(
                    f.default_color_order
                        .as_deref()
                        .unwrap_or(&crate::color::COLOR_ORDER),
                ),
                "defaultaxescolor" => PropValue::row(&[1.0, 1.0, 1.0]),
                "currentobject" => PropValue::Handles(Vec::new()),
                "nextplot" => PropValue::str(if f.hold_add { "add" } else { "replace" }),
                "currentaxes" => match f.current_axes {
                    Some(a) => PropValue::Handle(a),
                    None => PropValue::Handles(Vec::new()),
                },
                _ => return Err(bad()),
            },
            Kind::Axes(a) => return self.get_axes(h, a, &key, name),
            Kind::Line(l) => match key.as_str() {
                "xdata" => PropValue::row(&l.x),
                "ydata" => PropValue::row(&l.y),
                "zdata" => PropValue::row(&l.z),
                "color" => PropValue::row(&l.color),
                "linestyle" => PropValue::str(&l.line_style),
                "linewidth" => PropValue::Num(l.line_width),
                "marker" => PropValue::str(&l.marker),
                "markersize" => PropValue::Num(l.marker_size),
                "markeredgecolor" => paint_value(l.marker_edge),
                "markerfacecolor" => paint_value(l.marker_face),
                _ => return Err(bad()),
            },
            Kind::Scatter(s) => match key.as_str() {
                "xdata" => PropValue::row(&s.x),
                "ydata" => PropValue::row(&s.y),
                "zdata" => PropValue::row(&s.z),
                "sizedata" => PropValue::row(&s.size_data),
                "cdata" => match &s.cdata {
                    ScatterColor::Rgb(c) => PropValue::row(c),
                    ScatterColor::Indexed(v) => PropValue::Num(*v),
                    ScatterColor::PerPoint(a) => PropValue::Array(a.clone()),
                },
                "marker" => PropValue::str(&s.marker),
                "markeredgecolor" => paint_value(s.marker_edge),
                "markerfacecolor" => paint_value(s.marker_face),
                "linewidth" => PropValue::Num(s.line_width),
                _ => return Err(bad()),
            },
            Kind::Surface(s) => match key.as_str() {
                "xdata" => PropValue::Array(s.x.clone()),
                "ydata" => PropValue::Array(s.y.clone()),
                "zdata" => PropValue::Array(s.z.clone()),
                "cdata" => PropValue::Array(s.c.clone()),
                "facecolor" => paint_value(s.face_color),
                "edgecolor" => paint_value(s.edge_color),
                "meshstyle" => PropValue::str(&s.mesh_style),
                "linestyle" => PropValue::str(&s.line_style),
                "linewidth" => PropValue::Num(s.line_width),
                "cdatamapping" => PropValue::str(if s.cdata_direct { "direct" } else { "scaled" }),
                "facealpha" => PropValue::Num(s.face_alpha),
                "edgealpha" => PropValue::Num(s.edge_alpha),
                "marker" => PropValue::str(&s.marker),
                _ => return Err(bad()),
            },
            Kind::Patch(p) => match key.as_str() {
                "vertices" => PropValue::Array(p.vertices.clone()),
                "faces" => PropValue::Array(p.faces.clone()),
                "facevertexcdata" => PropValue::Array(p.fvcdata.clone()),
                "xdata" | "ydata" => {
                    let c = usize::from(key == "ydata");
                    PropValue::col(p.vertices.column(c))
                }
                "facecolor" => paint_value(p.face_color),
                "edgecolor" => paint_value(p.edge_color),
                "cdatamapping" => PropValue::str(if p.cdata_direct { "direct" } else { "scaled" }),
                "linestyle" => PropValue::str(&p.line_style),
                "linewidth" => PropValue::Num(p.line_width),
                "marker" => PropValue::str(&p.marker),
                "facealpha" => PropValue::Num(p.face_alpha),
                "edgealpha" => PropValue::Num(p.edge_alpha),
                _ => return Err(bad()),
            },
            Kind::Text(t) => match key.as_str() {
                "string" => {
                    if t.lines.len() == 1 {
                        PropValue::str(&t.lines[0])
                    } else if t.lines.is_empty() {
                        PropValue::str("")
                    } else {
                        PropValue::Cell(t.lines.clone())
                    }
                }
                "position" => PropValue::row(&t.position),
                "units" => PropValue::str(&t.units),
                "fontsize" => PropValue::Num(t.font_size),
                "fontweight" => PropValue::str(&t.font_weight),
                "fontangle" => PropValue::str(&t.font_angle),
                "color" => PropValue::row(&t.color),
                "backgroundcolor" => paint_value(t.background),
                "edgecolor" => paint_value(t.edge_color),
                "margin" => PropValue::Num(t.margin),
                "horizontalalignment" => PropValue::str(&t.horizontal_alignment),
                "verticalalignment" => PropValue::str(&t.vertical_alignment),
                "rotation" => PropValue::Num(t.rotation),
                "interpreter" => PropValue::str(&t.interpreter),
                "linestyle" => PropValue::str(&t.line_style),
                "linewidth" => PropValue::Num(t.line_width),
                "clipping" => PropValue::on_off(t.clipping),
                _ => return Err(bad()),
            },
            Kind::Image(i) => match key.as_str() {
                "cdata" => PropValue::Array(i.cdata.clone()),
                "xdata" => PropValue::row(&i.x),
                "ydata" => PropValue::row(&i.y),
                "cdatamapping" => PropValue::str(if i.cdata_direct { "direct" } else { "scaled" }),
                _ => return Err(bad()),
            },
            Kind::Group(g) => get_group(&g.kind, &key).ok_or_else(bad)?,
            Kind::Legend(l) => match key.as_str() {
                "string" => PropValue::Cell(l.labels.clone()),
                "location" => PropValue::str(&l.location),
                _ => return Err(bad()),
            },
            Kind::Colorbar(c) => match key.as_str() {
                "location" => PropValue::str(&c.location),
                "label" => PropValue::str(&c.label),
                "limits" => {
                    let st = self
                        .axes_state(c.peer)
                        .ok_or_else(GraphicsError::invalid_handle)?;
                    PropValue::row(&st.clim)
                }
                _ => return Err(bad()),
            },
        })
    }

    fn get_axes(
        &self,
        h: Handle,
        a: &crate::model::Axes,
        key: &str,
        name: &str,
    ) -> Result<PropValue> {
        let st = self
            .axes_state(h)
            .ok_or_else(GraphicsError::invalid_handle)?;
        let axis_index = |c: u8| match c {
            b'x' => Some(0),
            b'y' => Some(1),
            b'z' => Some(2),
            _ => None,
        };
        if let Some(i) = axis_index(key.as_bytes()[0]) {
            let p = a.axis(i);
            let out = match &key[1..] {
                "lim" => Some(PropValue::row(&st.lim[i])),
                "limmode" => Some(PropValue::str(p.lim_mode.name())),
                "tick" => Some(PropValue::row(&st.tick[i])),
                "tickmode" => Some(PropValue::str(p.tick_mode.name())),
                "ticklabel" => Some(PropValue::Cell(st.ticklabel[i].clone())),
                "ticklabelmode" => Some(PropValue::str(p.ticklabel_mode.name())),
                "scale" => Some(PropValue::str(if p.scale == Scale::Log {
                    "log"
                } else {
                    "linear"
                })),
                "dir" => Some(PropValue::str(if p.reverse { "reverse" } else { "normal" })),
                "grid" => Some(PropValue::on_off(p.grid)),
                "minorgrid" => Some(PropValue::on_off(p.minor_grid)),
                "label" => Some(PropValue::Handle(p.label)),
                _ => None,
            };
            if let Some(v) = out {
                return Ok(v);
            }
        }
        Ok(match key {
            "title" => PropValue::Handle(a.title),
            "view" => PropValue::row(&a.view),
            "clim" => PropValue::row(&st.clim),
            "climmode" => PropValue::str(a.clim_mode.name()),
            "dataaspectratio" => PropValue::row(&st.dar),
            "dataaspectratiomode" => PropValue::str(a.dar_mode.name()),
            "plotboxaspectratio" => PropValue::row(&st.pbar),
            "plotboxaspectratiomode" => PropValue::str(a.pbar_mode.name()),
            "cameraposition" => PropValue::row(&st.camera.position),
            "cameratarget" => PropValue::row(&st.camera.target),
            "cameraupvector" => PropValue::row(&st.camera.up),
            "cameraviewangle" => PropValue::Num(st.camera.view_angle),
            "colormap" => colormap_value(&st.colormap),
            "colororder" => colormap_value(&a.color_order),
            "colororderindex" => PropValue::Num((a.color_order_index + 1) as f64),
            "nextplot" => PropValue::str(if a.hold_add { "add" } else { "replace" }),
            "position" => PropValue::row(&self.inset_position(h, a.position)),
            "color" => paint_value(a.color),
            "box" => PropValue::on_off(a.box_on),
            "yaxislocation" => PropValue::str(if a.y_right { "right" } else { "left" }),
            "xaxislocation" => PropValue::str("bottom"),
            "fontsize" => PropValue::Num(a.font_size),
            "units" => PropValue::str("normalized"),
            "ticklabelinterpreter" => PropValue::str(&a.tick_label_interpreter),
            "projection" => PropValue::str("orthographic"),
            _ => return Err(GraphicsError::bad_property("axes", name)),
        })
    }
}

fn colormap_rows(v: &PropValue, name: &str) -> Result<Vec<Rgb>> {
    let a = array(v, name)?;
    if a.cols() != 3 || a.rows() == 0 || a.data.iter().any(|x| !(0.0..=1.0).contains(x)) {
        return Err(GraphicsError::bad_value(name));
    }
    Ok((0..a.rows())
        .map(|r| [a.at(r, 0), a.at(r, 1), a.at(r, 2)])
        .collect())
}

fn colormap_value(map: &[Rgb]) -> PropValue {
    let n = map.len();
    let mut data = Vec::with_capacity(3 * n);
    for c in 0..3 {
        data.extend(map.iter().map(|r| r[c]));
    }
    PropValue::Array(Array::matrix(n, 3, data))
}

fn set_group(kind: &mut GroupKind, key: &str, name: &str, v: &PropValue) -> Result<()> {
    let bad = || GraphicsError::bad_property("hggroup", name);
    match kind {
        GroupKind::Bar {
            x,
            y,
            width,
            base_value,
            face_color,
            edge_color,
            line_width,
            line_style: ls,
            ..
        } => match key {
            "xdata" => *x = vector(v, name)?,
            "ydata" => *y = vector(v, name)?,
            "barwidth" => *width = positive(v, name)?,
            "basevalue" => *base_value = scalar(v, name)?,
            "facecolor" => *face_color = parse_paint(v, name, &["none", "flat"])?,
            "edgecolor" => *edge_color = parse_paint(v, name, &["none"])?,
            "linewidth" => *line_width = positive(v, name)?,
            "linestyle" => *ls = line_style(v, name)?,
            _ => return Err(bad()),
        },
        GroupKind::Area {
            face_color,
            edge_color,
            line_width,
            line_style: ls,
            ..
        } => match key {
            "facecolor" => *face_color = parse_paint(v, name, &["none"])?,
            "edgecolor" => *edge_color = parse_paint(v, name, &["none"])?,
            "linewidth" => *line_width = positive(v, name)?,
            "linestyle" => *ls = line_style(v, name)?,
            _ => return Err(bad()),
        },
        GroupKind::Stem(l, base) => {
            if key == "basevalue" {
                *base = scalar(v, name)?;
            } else {
                set_line_like(l, key, name, v)?;
            }
        }
        GroupKind::Stairs(l) => set_line_like(l, key, name, v)?,
        GroupKind::Histogram(h) => set_histogram(h, key, name, v)?,
        GroupKind::ErrorBar(e) => crate::lines2::set_errorbar(e, key, name, v)?,
        GroupKind::Quiver(q) => crate::lines2::set_quiver(q, key, name, v)?,
        GroupKind::FunctionLine(f) => crate::lines2::set_function_line(f, key, name, v)?,
        GroupKind::PolarGrid => return Err(bad()),
        GroupKind::Contour {
            levels,
            level_mode,
            line_color,
            line_style: ls,
            line_width,
            fill,
            ..
        } => match key {
            "levellist" => {
                let mut l = vector(v, name)?;
                l.sort_by(f64::total_cmp);
                *levels = l;
                *level_mode = Mode::Manual;
            }
            "levellistmode" => *level_mode = mode(v, name)?,
            "linecolor" | "edgecolor" => *line_color = parse_paint(v, name, &["none", "flat"])?,
            "linestyle" => *ls = line_style(v, name)?,
            "linewidth" => *line_width = positive(v, name)?,
            "fill" => *fill = on_off(v, name)?,
            _ => return Err(bad()),
        },
        GroupKind::Contour3(_) if key == "fill" => {
            return Err(crate::surf2::not_yet(
                "set",
                "'Fill' on a 3-D contour group",
            ));
        }
        GroupKind::Contour3(c) => set_group(&mut c.contour, key, name, v)?,
    }
    Ok(())
}

fn unit(v: &PropValue, name: &str) -> Result<f64> {
    let x = scalar(v, name)?;
    if (0.0..=1.0).contains(&x) {
        Ok(x)
    } else {
        Err(GraphicsError::bad_value(name))
    }
}

fn set_histogram(
    h: &mut crate::model::Histogram,
    key: &str,
    name: &str,
    v: &PropValue,
) -> Result<()> {
    match key {
        "data" => {
            h.data = vector(v, name)?;
            h.recount();
        }
        "binedges" => {
            let e = vector(v, name)?;
            if e.len() < 2 || e.iter().any(|x| x.is_nan()) || e.windows(2).any(|w| w[0] > w[1]) {
                return Err(GraphicsError::bad_value(name));
            }
            h.edges = e;
            h.bin_method = "manual".into();
            h.recount();
        }
        // The interpreter chooses the edges (`histcounts`) and then records
        // the method here; the model cannot re-bin by itself.
        "binmethod" => h.bin_method = string(v, name)?.to_ascii_lowercase(),
        "numbins" | "binwidth" | "binlimits" => {
            return Err(GraphicsError::args(
                "set",
                format!("{name} of a histogram is set through the interpreter (histcounts)"),
            ));
        }
        "normalization" => {
            h.normalization = choice(
                v,
                name,
                &[
                    "count",
                    "probability",
                    "pdf",
                    "cdf",
                    "countdensity",
                    "cumcount",
                ],
            )?
        }
        "orientation" => {
            h.horizontal = choice(v, name, &["vertical", "horizontal"])? == "horizontal"
        }
        "displaystyle" => h.stairs = choice(v, name, &["bar", "stairs"])? == "stairs",
        "facecolor" => h.face_color = parse_paint(v, name, &["none", "auto"])?,
        "edgecolor" => h.edge_color = parse_paint(v, name, &["none", "auto"])?,
        "facealpha" => h.face_alpha = unit(v, name)?,
        "edgealpha" => h.edge_alpha = unit(v, name)?,
        "linewidth" => h.line_width = positive(v, name)?,
        "linestyle" => h.line_style = line_style(v, name)?,
        _ => return Err(GraphicsError::bad_property("histogram", name)),
    }
    Ok(())
}

fn get_histogram(h: &crate::model::Histogram, key: &str) -> Option<PropValue> {
    let e = &h.edges;
    Some(match key {
        "data" => PropValue::row(&h.data),
        "values" => PropValue::row(&h.values()),
        "bincounts" => PropValue::row(&h.counts),
        "binedges" => PropValue::row(e),
        "numbins" => PropValue::Num(e.len().saturating_sub(1) as f64),
        "binwidth" => {
            let w: Vec<f64> = e.windows(2).map(|w| w[1] - w[0]).collect();
            let first = w.first().copied().unwrap_or(0.0);
            let tol = 1e-10 * first.abs().max(f64::MIN_POSITIVE);
            if w.iter().all(|x| (x - first).abs() <= tol) {
                PropValue::Num(first)
            } else {
                PropValue::str("nonuniform")
            }
        }
        "binlimits" => PropValue::row(&[e[0], e[e.len() - 1]]),
        "binlimitsmode" => PropValue::str(if h.bin_method == "manual" {
            "manual"
        } else {
            "auto"
        }),
        "binmethod" => PropValue::str(&h.bin_method),
        "normalization" => PropValue::str(&h.normalization),
        "orientation" => PropValue::str(if h.horizontal {
            "horizontal"
        } else {
            "vertical"
        }),
        "displaystyle" => PropValue::str(if h.stairs { "stairs" } else { "bar" }),
        "facecolor" => paint_value(h.face_color),
        "edgecolor" => paint_value(h.edge_color),
        "facealpha" => PropValue::Num(h.face_alpha),
        "edgealpha" => PropValue::Num(h.edge_alpha),
        "linewidth" => PropValue::Num(h.line_width),
        "linestyle" => PropValue::str(&h.line_style),
        _ => return None,
    })
}

fn set_line_like(l: &mut crate::model::Line, key: &str, name: &str, v: &PropValue) -> Result<()> {
    match key {
        "xdata" => l.x = vector(v, name)?,
        "ydata" => l.y = vector(v, name)?,
        "color" => l.color = rgb_value(v, name)?,
        "linestyle" => l.line_style = line_style(v, name)?,
        "linewidth" => l.line_width = positive(v, name)?,
        "marker" => l.marker = marker(v, name)?,
        "markersize" => l.marker_size = positive(v, name)?,
        "markeredgecolor" => l.marker_edge = parse_paint(v, name, &["none", "auto"])?,
        "markerfacecolor" => l.marker_face = parse_paint(v, name, &["none", "auto"])?,
        _ => return Err(GraphicsError::bad_property("hggroup", name)),
    }
    Ok(())
}

fn get_group(kind: &GroupKind, key: &str) -> Option<PropValue> {
    Some(match kind {
        GroupKind::Bar {
            x,
            y,
            horizontal,
            stacked,
            width,
            base_value,
            face_color,
            edge_color,
            line_width,
            line_style,
            ..
        } => match key {
            "xdata" => PropValue::row(x),
            "ydata" => PropValue::row(y),
            "horizontal" => PropValue::on_off(*horizontal),
            "barlayout" => PropValue::str(if *stacked { "stacked" } else { "grouped" }),
            "barwidth" => PropValue::Num(*width),
            "basevalue" => PropValue::Num(*base_value),
            "facecolor" => paint_value(*face_color),
            "edgecolor" => paint_value(*edge_color),
            "linewidth" => PropValue::Num(*line_width),
            "linestyle" => PropValue::str(line_style),
            _ => return None,
        },
        GroupKind::Area {
            x,
            y,
            base_value,
            face_color,
            edge_color,
            line_width,
            line_style,
            ..
        } => match key {
            "xdata" => PropValue::row(x),
            "ydata" => PropValue::row(y),
            "basevalue" => PropValue::Num(*base_value),
            "facecolor" => paint_value(*face_color),
            "edgecolor" => paint_value(*edge_color),
            "linewidth" => PropValue::Num(*line_width),
            "linestyle" => PropValue::str(line_style),
            _ => return None,
        },
        GroupKind::Histogram(h) => get_histogram(h, key)?,
        GroupKind::ErrorBar(e) => crate::lines2::get_errorbar(e, key)?,
        GroupKind::Quiver(q) => crate::lines2::get_quiver(q, key)?,
        GroupKind::FunctionLine(f) => crate::lines2::get_function_line(f, key)?,
        GroupKind::PolarGrid => return None,
        GroupKind::Stem(l, base) if key == "basevalue" => {
            let _ = l;
            PropValue::Num(*base)
        }
        GroupKind::Stem(l, _) | GroupKind::Stairs(l) => match key {
            "xdata" => PropValue::row(&l.x),
            "ydata" => PropValue::row(&l.y),
            "color" => PropValue::row(&l.color),
            "linestyle" => PropValue::str(&l.line_style),
            "linewidth" => PropValue::Num(l.line_width),
            "marker" => PropValue::str(&l.marker),
            "markersize" => PropValue::Num(l.marker_size),
            "markeredgecolor" => paint_value(l.marker_edge),
            "markerfacecolor" => paint_value(l.marker_face),
            _ => return None,
        },
        GroupKind::Contour {
            z,
            level_mode,
            line_color,
            line_style,
            line_width,
            fill,
            ..
        } => match key {
            "levellist" => PropValue::row(&crate::scene::contour_levels(kind)),
            "levellistmode" => PropValue::str(level_mode.name()),
            "linecolor" | "edgecolor" => paint_value(*line_color),
            "linestyle" => PropValue::str(line_style),
            "linewidth" => PropValue::Num(*line_width),
            "fill" => PropValue::on_off(*fill),
            "zdata" => PropValue::Array(z.clone()),
            _ => return None,
        },
        GroupKind::Contour3(c) => get_group(&c.contour, key)?,
    })
}

impl PropValue {
    pub fn col(v: &[f64]) -> PropValue {
        PropValue::Array(Array::col(v))
    }
}
