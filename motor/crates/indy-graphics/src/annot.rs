//! Axes annotation and configuration: `xlabel ylabel zlabel title legend
//! grid axis xlim ylim zlim caxis colormap colorbar view box`.

use crate::color::{Colormap, colormap_by_name};
use crate::error::{GraphicsError, Result};
use crate::graphics::{Graphics, Opts};
use crate::layout;
use crate::model::{Colorbar, Handle, Kind, LabelRole, Legend, Mode};

/// `axis` requests.
#[derive(Debug, Clone, PartialEq)]
pub enum AxisCmd {
    /// `axis([xmin xmax ymin ymax [zmin zmax]])`.
    Limits(Vec<f64>),
    Tight,
    Equal,
    /// `axis image`: equal data units, tight limits, automatic plot box.
    Image,
    Normal,
    Auto,
    Off,
    On,
    Square,
    /// `axis ij` (y downward) / `axis xy`.
    Ij,
    Xy,
}

/// Where `colormap` puts the map.
#[derive(Debug, Clone, PartialEq)]
pub enum ColormapArg {
    Name(String, Option<usize>),
    Rows(Colormap),
}

/// The legend location names accepted by `legend(..., 'Location', loc)`.
pub const LEGEND_LOCATIONS: [&str; 19] = [
    "north",
    "south",
    "east",
    "west",
    "northeast",
    "northwest",
    "southeast",
    "southwest",
    "northoutside",
    "southoutside",
    "eastoutside",
    "westoutside",
    "northeastoutside",
    "northwestoutside",
    "southeastoutside",
    "southwestoutside",
    "best",
    "bestoutside",
    "none",
];

impl Graphics {
    fn target_axes(&mut self, ax: Option<Handle>) -> Result<Handle> {
        match ax {
            Some(h) => {
                self.axes_ref(h)?;
                Ok(h)
            }
            None => Ok(self.gca()),
        }
    }

    /// `xlabel`/`ylabel`/`zlabel`/`title`: returns the label's text handle.
    pub fn label(
        &mut self,
        ax: Option<Handle>,
        role: LabelRole,
        lines: &[String],
        opts: &Opts,
    ) -> Result<Handle> {
        let ax = self.target_axes(ax)?;
        let h = self.label_handle(ax, role)?;
        self.text_mut(h)?.lines = lines.to_vec();
        self.apply_opts(&[h], opts)?;
        Ok(h)
    }

    /// `legend(labels...)`, labelling the first plot children in creation
    /// order; also sets their `DisplayName`. With no labels, uses existing
    /// display names (`dataN` where empty).
    pub fn legend(
        &mut self,
        ax: Option<Handle>,
        labels: Option<&[String]>,
        location: Option<&str>,
    ) -> Result<Handle> {
        let ax = self.target_axes(ax)?;
        let location = match location {
            Some(l) => {
                let l = l.to_ascii_lowercase();
                if !LEGEND_LOCATIONS.contains(&l.as_str()) {
                    return Err(GraphicsError::args(
                        "legend",
                        format!("unknown location '{l}'"),
                    ));
                }
                l
            }
            None => "northeast".to_string(),
        };
        let items: Vec<Handle> = self
            .obj(ax)?
            .children
            .iter()
            .copied()
            .filter(|h| self.legend_capable(*h))
            .collect();
        let labels: Vec<String> = match labels {
            Some(l) => {
                if l.len() > items.len() {
                    // MATLAB warns and ignores extra labels.
                    l[..items.len()].to_vec()
                } else {
                    l.to_vec()
                }
            }
            None => items
                .iter()
                .enumerate()
                .map(|(i, h)| {
                    let name = self.display_name(*h);
                    if name.is_empty() {
                        format!("data{}", i + 1)
                    } else {
                        name
                    }
                })
                .collect(),
        };
        for (h, name) in items.iter().zip(&labels) {
            self.set_display_name(*h, name);
        }
        if let Some(old) = self.axes_ref(ax)?.legend {
            self.delete_tree(old);
        }
        let fig = self.ancestor_figure(ax)?;
        let h = self.add(
            fig,
            Kind::Legend(Legend {
                peer: ax,
                labels,
                location,
            }),
        );
        self.axes_mut(ax)?.legend = Some(h);
        Ok(h)
    }

    /// `legend off` / `legend hide` (Some(false)), `legend show` (Some(true)).
    pub fn legend_visible(&mut self, ax: Option<Handle>, visible: bool) -> Result<()> {
        let ax = self.target_axes(ax)?;
        if let Some(l) = self.axes_ref(ax)?.legend {
            if visible {
                self.obj_mut(l)?.visible = true;
            } else {
                self.delete_tree(l);
            }
        } else if visible {
            self.legend(Some(ax), None, None)?;
        }
        Ok(())
    }

    fn legend_capable(&self, h: Handle) -> bool {
        if crate::lines2::is_polar_grid(self, h) {
            return false;
        }
        matches!(
            self.objects.get(&h).map(|o| &o.kind),
            Some(
                Kind::Line(_)
                    | Kind::Scatter(_)
                    | Kind::Surface(_)
                    | Kind::Patch(_)
                    | Kind::Group(_)
            )
        )
    }

    pub(crate) fn display_name(&self, h: Handle) -> String {
        match self.objects.get(&h).map(|o| &o.kind) {
            Some(Kind::Line(l)) => l.display_name.clone(),
            Some(Kind::Scatter(s)) => s.display_name.clone(),
            Some(Kind::Surface(s)) => s.display_name.clone(),
            Some(Kind::Patch(p)) => p.display_name.clone(),
            Some(Kind::Image(i)) => i.display_name.clone(),
            Some(Kind::Group(g)) => g.display_name.clone(),
            _ => String::new(),
        }
    }

    pub(crate) fn set_display_name(&mut self, h: Handle, name: &str) {
        let name = name.to_string();
        match self.objects.get_mut(&h).map(|o| &mut o.kind) {
            Some(Kind::Line(l)) => l.display_name = name,
            Some(Kind::Scatter(s)) => s.display_name = name,
            Some(Kind::Surface(s)) => s.display_name = name,
            Some(Kind::Patch(p)) => p.display_name = name,
            Some(Kind::Image(i)) => i.display_name = name,
            Some(Kind::Group(g)) => g.display_name = name,
            _ => {}
        }
    }

    /// `grid on/off` (Some), `grid` toggles (None); `minor` for `grid minor`.
    pub fn grid(&mut self, ax: Option<Handle>, state: Option<bool>, minor: bool) -> Result<()> {
        let ax = self.target_axes(ax)?;
        let a = self.axes_mut(ax)?;
        let on = state.unwrap_or(!a.x.grid);
        for i in 0..3 {
            let p = a.axis_mut(i);
            if minor {
                p.minor_grid = on;
            } else {
                p.grid = on;
            }
        }
        Ok(())
    }

    /// `box on/off`.
    pub fn box_(&mut self, ax: Option<Handle>, state: Option<bool>) -> Result<()> {
        let ax = self.target_axes(ax)?;
        let a = self.axes_mut(ax)?;
        a.box_on = state.unwrap_or(!a.box_on);
        Ok(())
    }

    /// `xlim([a b])` (i = 0), `ylim` (1), `zlim` (2); `None` = `xlim auto`.
    pub fn set_limits(
        &mut self,
        ax: Option<Handle>,
        axis: usize,
        lim: Option<[f64; 2]>,
    ) -> Result<()> {
        let ax = self.target_axes(ax)?;
        let names = ["xlim", "ylim", "zlim"];
        let a = self.axes_mut(ax)?;
        let p = a.axis_mut(axis.min(2));
        match lim {
            Some(l) => {
                // MATLAB allows -Inf/Inf to keep the automatic end.
                if l[0].is_nan() || l[1].is_nan() || l[0] >= l[1] {
                    return Err(GraphicsError::args(
                        names[axis.min(2)],
                        "limits must be a 2-element vector of increasing numbers",
                    ));
                }
                p.lim = l;
                p.lim_mode = Mode::Manual;
            }
            None => p.lim_mode = Mode::Auto,
        }
        if lim.is_some_and(|l| l.iter().any(|v| v.is_infinite())) {
            let st = self
                .axes_state(ax)
                .ok_or_else(GraphicsError::invalid_handle)?;
            let auto = st.lim[axis.min(2)];
            let a = self.axes_mut(ax)?;
            let p = a.axis_mut(axis.min(2));
            let l = lim.unwrap_or(auto);
            p.lim = [
                if l[0].is_infinite() { auto[0] } else { l[0] },
                if l[1].is_infinite() { auto[1] } else { l[1] },
            ];
        }
        Ok(())
    }

    /// Current limits of an axis (resolving automatic ones).
    pub fn limits(&mut self, ax: Option<Handle>, axis: usize) -> Result<[f64; 2]> {
        let ax = self.target_axes(ax)?;
        let st = self
            .axes_state(ax)
            .ok_or_else(GraphicsError::invalid_handle)?;
        Ok(st.lim[axis.min(2)])
    }

    /// `axis(...)`.
    pub fn axis(&mut self, ax: Option<Handle>, cmd: &AxisCmd) -> Result<()> {
        let ax = self.target_axes(ax)?;
        match cmd {
            AxisCmd::Limits(v) => {
                if !(v.len() == 4 || v.len() == 6) {
                    return Err(GraphicsError::args(
                        "axis",
                        "LIMITS must have 4 or 6 elements",
                    ));
                }
                for (i, pair) in v.chunks(2).enumerate() {
                    self.set_limits(Some(ax), i, Some([pair[0], pair[1]]))?;
                }
            }
            AxisCmd::Tight => {
                let a = self.axes_mut(ax)?;
                a.tight = true;
                for i in 0..3 {
                    a.axis_mut(i).lim_mode = Mode::Auto;
                }
            }
            AxisCmd::Auto => {
                let a = self.axes_mut(ax)?;
                a.tight = false;
                for i in 0..3 {
                    a.axis_mut(i).lim_mode = Mode::Auto;
                }
            }
            AxisCmd::Equal => {
                let fig = self.ancestor_figure(ax)?;
                let size = {
                    let f = self.figure_ref(fig)?;
                    [f.position[2], f.position[3]]
                };
                let a = self.axes_mut(ax)?;
                a.dar = [1.0, 1.0, 1.0];
                a.dar_mode = Mode::Manual;
                // Measured: a 2-D axes takes its pixel box as aspect.
                a.pbar = if a.is_2d_view() {
                    let w = a.position[2] * size[0];
                    let h = a.position[3] * size[1];
                    [w, h, h]
                } else {
                    [1.0, 1.0, 1.0]
                };
                a.pbar_mode = Mode::Manual;
            }
            AxisCmd::Image => {
                // Measured: the plot box follows the tight data ranges
                // (`imagesc(10×3); axis image` → PBAR [1.5 5 1]).
                let a = self.axes_mut(ax)?;
                a.dar = [1.0, 1.0, 1.0];
                a.dar_mode = Mode::Manual;
                a.pbar = [1.0, 1.0, 1.0];
                a.pbar_mode = Mode::Auto;
                a.tight = true;
                for i in 0..3 {
                    a.axis_mut(i).lim_mode = Mode::Auto;
                }
            }
            AxisCmd::Square => {
                let a = self.axes_mut(ax)?;
                a.pbar = [1.0, 1.0, 1.0];
                a.pbar_mode = Mode::Manual;
            }
            AxisCmd::Normal => {
                let a = self.axes_mut(ax)?;
                a.dar_mode = Mode::Auto;
                a.pbar = [1.0, 1.0, 1.0];
                a.pbar_mode = Mode::Auto;
            }
            AxisCmd::Off => self.obj_mut(ax)?.visible = false,
            AxisCmd::On => self.obj_mut(ax)?.visible = true,
            AxisCmd::Ij => self.axes_mut(ax)?.y.reverse = true,
            AxisCmd::Xy => self.axes_mut(ax)?.y.reverse = false,
        }
        Ok(())
    }

    /// `caxis([a b])` / `clim`; `None` restores automatic colour limits.
    pub fn caxis(&mut self, ax: Option<Handle>, lim: Option<[f64; 2]>) -> Result<()> {
        let ax = self.target_axes(ax)?;
        let a = self.axes_mut(ax)?;
        match lim {
            Some(l) => {
                if !(l[0].is_finite() && l[1].is_finite() && l[0] < l[1]) {
                    return Err(GraphicsError::args(
                        "caxis",
                        "limits must be increasing and finite",
                    ));
                }
                a.clim = l;
                a.clim_mode = Mode::Manual;
            }
            None => a.clim_mode = Mode::Auto,
        }
        Ok(())
    }

    /// `colormap(name)` / `colormap(map)`; sets the figure map like MATLAB
    /// (or an axes map when `ax` is given). Returns the map now in effect.
    pub fn colormap(&mut self, ax: Option<Handle>, arg: Option<&ColormapArg>) -> Result<Colormap> {
        let map = match arg {
            None => None,
            Some(ColormapArg::Name(name, n)) => {
                // Measured: `gray()` takes the length of the current map.
                let fig = match ax {
                    Some(a) => self.ancestor_figure(a)?,
                    None => self.gcf(),
                };
                let n = n.unwrap_or(self.figure_ref(fig)?.colormap.len());
                Some(colormap_by_name(name, n).ok_or_else(|| {
                    GraphicsError::unsupported("colormap", format!("colormap '{name}'"))
                })?)
            }
            Some(ColormapArg::Rows(rows)) => {
                if rows.is_empty() || rows.iter().flatten().any(|v| !(0.0..=1.0).contains(v)) {
                    return Err(GraphicsError::args(
                        "colormap",
                        "MAP must be an N×3 matrix with values in [0,1]",
                    ));
                }
                Some(rows.clone())
            }
        };
        match ax {
            Some(axh) => {
                let a = self.axes_mut(axh)?;
                if let Some(m) = map {
                    a.colormap = Some(m);
                }
                let fig = self.ancestor_figure(axh)?;
                let fallback = self.figure_ref(fig)?.colormap.clone();
                Ok(self.axes_ref(axh)?.colormap.clone().unwrap_or(fallback))
            }
            None => {
                let fig = self.gcf();
                if let Some(m) = map {
                    self.figure_mut(fig)?.colormap = m;
                    // Axes without their own map follow the figure.
                }
                Ok(self.figure_ref(fig)?.colormap.clone())
            }
        }
    }

    /// `colorbar` / `colorbar(location)`; `colorbar off` is [`Graphics::delete`]
    /// on the returned handle or [`Graphics::colorbar_off`].
    pub fn colorbar(&mut self, ax: Option<Handle>, location: Option<&str>) -> Result<Handle> {
        let ax = self.target_axes(ax)?;
        let location = location.unwrap_or("eastoutside").to_ascii_lowercase();
        let base = self.axes_ref(ax)?.base_position;
        let (peer_pos, _, _) = layout::colorbar_geometry(&location, base).ok_or_else(|| {
            GraphicsError::args("colorbar", format!("unknown location '{location}'"))
        })?;
        if let Some(old) = self.axes_ref(ax)?.colorbar {
            self.delete_tree(old);
        }
        let fig = self.ancestor_figure(ax)?;
        let h = self.add(
            fig,
            Kind::Colorbar(Colorbar {
                peer: ax,
                location,
                label: String::new(),
            }),
        );
        let a = self.axes_mut(ax)?;
        a.colorbar = Some(h);
        a.position = peer_pos;
        Ok(h)
    }

    pub fn colorbar_off(&mut self, ax: Option<Handle>) -> Result<()> {
        let ax = self.target_axes(ax)?;
        if let Some(cb) = self.axes_ref(ax)?.colorbar {
            self.delete_tree(cb);
        }
        Ok(())
    }

    /// `view(az, el)`; `view(2)` is `[0 90]`, `view(3)` is `[-37.5 30]`.
    pub fn view(&mut self, ax: Option<Handle>, az_el: [f64; 2]) -> Result<()> {
        if !(az_el[0].is_finite() && az_el[1].is_finite()) {
            return Err(GraphicsError::args("view", "AZ and EL must be finite"));
        }
        let ax = self.target_axes(ax)?;
        self.axes_mut(ax)?.view = az_el;
        Ok(())
    }
}
