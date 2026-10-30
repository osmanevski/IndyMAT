//! The graphics session: object storage and the figure/axes lifecycle
//! (`figure`, `gcf`, `gca`, `axes`, `hold`, `clf`, `close`, `subplot`).

use std::collections::HashMap;

use crate::error::{GraphicsError, Result};
use crate::layout;
use crate::model::{AppData, Axes, Figure, GridSlot, Handle, Kind, LabelRole, Object, Text};
use crate::value::PropValue;

/// Name/value property pairs passed after the positional arguments.
pub type Opts = [(String, PropValue)];

/// Builds one option pair (`opt("LineWidth", 2.0)`).
pub fn opt(name: &str, value: impl Into<PropValue>) -> (String, PropValue) {
    (name.to_string(), value.into())
}

/// Which figures `close` removes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseTarget {
    Current,
    All,
    Handle(Handle),
}

/// One graphics session (the root object and everything below it).
#[derive(Debug, Clone)]
pub struct Graphics {
    pub(crate) objects: HashMap<Handle, Object>,
    pub(crate) current_figure: Option<Handle>,
    next_object: i64,
    /// Figures in creation order (children of root).
    pub(crate) figures: Vec<Handle>,
    /// Active `tiledlayout` grid of each figure: rows, cols, next tile.
    pub(crate) tiles: HashMap<Handle, (usize, usize, usize)>,
    /// Last exported JSON per figure, for [`Graphics::take_changed`].
    pub(crate) exported: HashMap<Handle, String>,
    /// Application data of the root object (`setappdata(0, ...)`).
    root_appdata: AppData,
}

impl Default for Graphics {
    fn default() -> Self {
        Graphics::new()
    }
}

impl Graphics {
    pub fn new() -> Graphics {
        Graphics {
            objects: HashMap::new(),
            current_figure: None,
            next_object: 0,
            figures: Vec::new(),
            tiles: HashMap::new(),
            exported: HashMap::new(),
            root_appdata: AppData::default(),
        }
    }

    /// Whether `h` is an object handle this session created, live or
    /// deleted (object handles are never reused; figure numbers are, so
    /// they never count).
    pub fn was_object(&self, h: Handle) -> bool {
        h.0 < 0 && h.0 >= self.next_object
    }

    /// Application data of a live object (the root included).
    pub fn appdata(&self, h: Handle) -> Result<&AppData> {
        if h == Handle::ROOT {
            return Ok(&self.root_appdata);
        }
        Ok(&self.obj(h)?.appdata)
    }

    pub fn appdata_mut(&mut self, h: Handle) -> Result<&mut AppData> {
        if h == Handle::ROOT {
            return Ok(&mut self.root_appdata);
        }
        Ok(&mut self.obj_mut(h)?.appdata)
    }

    pub fn is_valid(&self, h: Handle) -> bool {
        h == Handle::ROOT || self.objects.contains_key(&h)
    }

    pub(crate) fn obj(&self, h: Handle) -> Result<&Object> {
        self.objects
            .get(&h)
            .ok_or_else(GraphicsError::invalid_handle)
    }

    pub(crate) fn obj_mut(&mut self, h: Handle) -> Result<&mut Object> {
        self.objects
            .get_mut(&h)
            .ok_or_else(GraphicsError::invalid_handle)
    }

    pub(crate) fn axes_ref(&self, h: Handle) -> Result<&Axes> {
        match &self.obj(h)?.kind {
            Kind::Axes(a) => Ok(a),
            _ => Err(GraphicsError::new(
                "MATLAB:hg:InvalidHandle",
                "handle is not an axes",
            )),
        }
    }

    pub(crate) fn axes_mut(&mut self, h: Handle) -> Result<&mut Axes> {
        match &mut self.obj_mut(h)?.kind {
            Kind::Axes(a) => Ok(a),
            _ => Err(GraphicsError::new(
                "MATLAB:hg:InvalidHandle",
                "handle is not an axes",
            )),
        }
    }

    pub(crate) fn figure_ref(&self, h: Handle) -> Result<&Figure> {
        match &self.obj(h)?.kind {
            Kind::Figure(f) => Ok(f),
            _ => Err(GraphicsError::new(
                "MATLAB:hg:InvalidHandle",
                "handle is not a figure",
            )),
        }
    }

    pub(crate) fn figure_mut(&mut self, h: Handle) -> Result<&mut Figure> {
        match &mut self.obj_mut(h)?.kind {
            Kind::Figure(f) => Ok(f),
            _ => Err(GraphicsError::new(
                "MATLAB:hg:InvalidHandle",
                "handle is not a figure",
            )),
        }
    }

    /// Type name of a handle (`'figure'`, `'line'`, ... `'root'`).
    pub fn type_of(&self, h: Handle) -> Result<&'static str> {
        if h == Handle::ROOT {
            return Ok("root");
        }
        Ok(self.obj(h)?.kind.type_name())
    }

    pub(crate) fn add(&mut self, parent: Handle, kind: Kind) -> Handle {
        self.next_object -= 1;
        let h = Handle(self.next_object);
        self.insert(h, parent, kind);
        h
    }

    fn insert(&mut self, h: Handle, parent: Handle, kind: Kind) {
        self.objects.insert(
            h,
            Object {
                parent,
                children: Vec::new(),
                visible: true,
                tag: String::new(),
                kind,
                appdata: Default::default(),
            },
        );
        if let Some(p) = self.objects.get_mut(&parent) {
            p.children.push(h);
        }
    }

    /// Parent of a handle (`Handle::ROOT` for figures).
    pub fn parent_of(&self, h: Handle) -> Result<Handle> {
        Ok(self.obj(h)?.parent)
    }

    /// The figure containing `h` (itself for a figure).
    pub fn ancestor_figure(&self, mut h: Handle) -> Result<Handle> {
        loop {
            let o = self.obj(h)?;
            if matches!(o.kind, Kind::Figure(_)) {
                return Ok(h);
            }
            h = o.parent;
        }
    }

    /// The axes containing `h` (itself for an axes).
    pub fn ancestor_axes(&self, mut h: Handle) -> Option<Handle> {
        loop {
            let o = self.objects.get(&h)?;
            match o.kind {
                Kind::Axes(_) => return Some(h),
                Kind::Figure(_) => return None,
                _ => h = o.parent,
            }
        }
    }

    // ----- figures ---------------------------------------------------------

    /// `figure` (new figure, lowest free number) or `figure(n)` (make figure
    /// `n` current, creating it if needed).
    pub fn figure(&mut self, number: Option<f64>) -> Result<Handle> {
        let n = match number {
            Some(v) => {
                if !(v.is_finite() && v >= 1.0 && v.fract() == 0.0) {
                    return Err(GraphicsError::new(
                        "MATLAB:figure:InvalidNumber",
                        "figure: N must be a positive integer figure number",
                    ));
                }
                v as i64
            }
            None => (1..)
                .find(|n| !self.objects.contains_key(&Handle(*n)))
                .unwrap_or(1),
        };
        let h = Handle(n);
        if !self.objects.contains_key(&h) {
            self.insert(h, Handle::ROOT, Kind::Figure(Figure::new(n)));
            self.figures.push(h);
        }
        self.current_figure = Some(h);
        Ok(h)
    }

    /// Current figure, created when none exists (MATLAB `gcf`).
    pub fn gcf(&mut self) -> Handle {
        match self.current_figure {
            Some(h) => h,
            None => self.figure(None).unwrap_or(Handle(1)),
        }
    }

    /// Current figure without creating one (`get(groot,'CurrentFigure')`).
    pub fn current_figure(&self) -> Option<Handle> {
        self.current_figure
    }

    /// All open figures, ordered by figure number.
    pub fn figure_handles(&self) -> Vec<Handle> {
        let mut v = self.figures.clone();
        v.sort();
        v
    }

    /// Current axes of the current figure, creating both when needed (`gca`).
    pub fn gca(&mut self) -> Handle {
        let fig = self.gcf();
        if let Ok(Some(ax)) = self.figure_ref(fig).map(|f| f.current_axes)
            && self.objects.contains_key(&ax)
        {
            return ax;
        }
        self.new_axes(fig, None)
    }

    /// Current axes without creating anything.
    pub fn current_axes(&self) -> Option<Handle> {
        let f = self.current_figure?;
        self.figure_ref(f).ok()?.current_axes
    }

    /// `axes` / `axes('Position', p)`: a new axes in the current figure.
    pub fn axes(&mut self, position: Option<[f64; 4]>) -> Result<Handle> {
        if let Some(p) = position
            && !(p.iter().all(|v| v.is_finite()) && p[2] > 0.0 && p[3] > 0.0)
        {
            return Err(GraphicsError::bad_value("Position"));
        }
        let fig = self.gcf();
        Ok(self.new_axes(fig, position))
    }

    /// `axes(h)` / `figure(h)` on an existing object: make it current.
    pub fn make_current(&mut self, h: Handle) -> Result<()> {
        match self.obj(h)?.kind {
            Kind::Figure(_) => {
                self.current_figure = Some(h);
                Ok(())
            }
            Kind::Axes(_) => {
                let fig = self.ancestor_figure(h)?;
                self.figure_mut(fig)?.current_axes = Some(h);
                self.current_figure = Some(fig);
                Ok(())
            }
            _ => Err(GraphicsError::new(
                "MATLAB:hg:InvalidHandle",
                "only a figure or axes can be made current",
            )),
        }
    }

    pub(crate) fn new_axes(&mut self, fig: Handle, position: Option<[f64; 4]>) -> Handle {
        self.next_object -= 1;
        let ax = Handle(self.next_object);
        let mut labels = [Handle::ROOT; 4];
        for (i, role) in [
            LabelRole::XLabel,
            LabelRole::YLabel,
            LabelRole::ZLabel,
            LabelRole::Title,
        ]
        .into_iter()
        .enumerate()
        {
            self.next_object -= 1;
            labels[i] = Handle(self.next_object);
            // Labels are hidden children: inserted below without `children` link.
            self.objects.insert(
                labels[i],
                Object {
                    parent: ax,
                    children: Vec::new(),
                    visible: layout::label_visible(role),
                    tag: String::new(),
                    kind: Kind::Text(layout::label_text(role)),
                    appdata: Default::default(),
                },
            );
        }
        let mut axes = Axes::new(labels);
        axes.color_order = self.default_color_order(fig);
        if let Some(p) = position {
            axes.position = p;
            axes.base_position = p;
        }
        self.insert(ax, fig, Kind::Axes(Box::new(axes)));
        if let Ok(f) = self.figure_mut(fig) {
            f.current_axes = Some(ax);
        }
        ax
    }

    /// The colour order new axes of `fig` start with: the figure's
    /// `DefaultAxesColorOrder`, else the factory order.
    pub(crate) fn default_color_order(&self, fig: Handle) -> Vec<crate::color::Rgb> {
        self.figure_ref(fig)
            .ok()
            .and_then(|f| f.default_color_order.clone())
            .unwrap_or_else(|| crate::color::COLOR_ORDER.to_vec())
    }

    /// Resolves the axes a plotting command draws into and applies the hold
    /// state (MATLAB `newplot`): without hold the axes is cleared and reset.
    pub(crate) fn new_plot(&mut self, ax: Option<Handle>) -> Result<Handle> {
        let ax = match ax {
            Some(h) => {
                self.axes_ref(h)?;
                h
            }
            None => self.gca(),
        };
        if !self.axes_ref(ax)?.hold_add {
            self.reset_axes(ax)?;
        }
        Ok(ax)
    }

    /// Deletes the plot children and restores defaults except position,
    /// colorbar and grid slot (`cla reset`).
    pub(crate) fn reset_axes(&mut self, ax: Handle) -> Result<()> {
        let children = self.obj(ax)?.children.clone();
        for c in children {
            self.delete_tree(c);
        }
        let old = self.axes_ref(ax)?.clone();
        if let Some(l) = old.legend {
            self.delete_tree(l);
        }
        let labels = [old.x.label, old.y.label, old.z.label, old.title];
        for (h, role) in labels.iter().zip([
            LabelRole::XLabel,
            LabelRole::YLabel,
            LabelRole::ZLabel,
            LabelRole::Title,
        ]) {
            if let Ok(o) = self.obj_mut(*h) {
                o.kind = Kind::Text(layout::label_text(role));
                o.visible = layout::label_visible(role);
            }
        }
        let mut fresh = Axes::new(labels);
        if let Ok(fig) = self.ancestor_figure(ax) {
            fresh.color_order = self.default_color_order(fig);
        }
        fresh.position = old.position;
        fresh.base_position = old.base_position;
        fresh.colorbar = old.colorbar;
        fresh.slot = old.slot;
        fresh.sg_base = old.sg_base;
        // A yyaxis side keeps its pairing, side colour and ruler place.
        if old.yy_pair.is_some() {
            fresh.yy_pair = old.yy_pair;
            fresh.y_right = old.y_right;
            fresh.color_order = old.color_order.clone();
            fresh.color = old.color;
            fresh.x.ticklabel_mode = old.x.ticklabel_mode;
            fresh.x.ticklabel = old.x.ticklabel.clone();
        }
        *self.axes_mut(ax)? = fresh;
        let o = self.obj_mut(ax)?;
        o.visible = true;
        Ok(())
    }

    /// `cla`: delete the plot children of an axes (keeping its properties).
    pub fn cla(&mut self, ax: Option<Handle>) -> Result<()> {
        let ax = ax.unwrap_or_else(|| self.gca());
        let children = self.obj(ax)?.children.clone();
        for c in children {
            self.delete_tree(c);
        }
        let a = self.axes_mut(ax)?;
        a.color_order_index = 0;
        Ok(())
    }

    /// `hold on` (Some(true)), `hold off` (Some(false)) or `hold` (toggle).
    pub fn hold(&mut self, ax: Option<Handle>, state: Option<bool>) -> Result<()> {
        let ax = match ax {
            Some(h) => h,
            None => self.gca(),
        };
        let a = self.axes_mut(ax)?;
        let on = state.unwrap_or(!a.hold_add);
        a.hold_add = on;
        let fig = self.ancestor_figure(ax)?;
        self.figure_mut(fig)?.hold_add = on;
        Ok(())
    }

    pub fn ishold(&self, ax: Option<Handle>) -> bool {
        let ax = match ax.or_else(|| self.current_axes()) {
            Some(h) => h,
            None => return false,
        };
        self.axes_ref(ax).map(|a| a.hold_add).unwrap_or(false)
    }

    /// `delete(h)`.
    pub fn delete(&mut self, h: Handle) -> Result<()> {
        if h == Handle::ROOT {
            return Err(GraphicsError::new(
                "MATLAB:hg:InvalidHandle",
                "the root object cannot be deleted",
            ));
        }
        self.obj(h)?;
        self.delete_tree(h);
        Ok(())
    }

    pub(crate) fn delete_tree(&mut self, h: Handle) {
        let Some(o) = self.objects.remove(&h) else {
            return;
        };
        for c in o.children {
            self.delete_tree(c);
        }
        match o.kind {
            Kind::Axes(a) => {
                for l in [a.x.label, a.y.label, a.z.label, a.title] {
                    self.objects.remove(&l);
                }
                for extra in [a.legend, a.colorbar].into_iter().flatten() {
                    self.delete_tree(extra);
                }
            }
            Kind::Legend(l) => {
                if let Ok(a) = self.axes_mut(l.peer) {
                    a.legend = None;
                }
            }
            Kind::Colorbar(c) => {
                if let Ok(a) = self.axes_mut(c.peer) {
                    a.colorbar = None;
                    a.position = a.base_position;
                }
            }
            _ => {}
        }
        if let Some(p) = self.objects.get_mut(&o.parent) {
            p.children.retain(|c| *c != h);
            if let Kind::Figure(f) = &mut p.kind
                && f.current_axes == Some(h)
            {
                f.current_axes = None;
            }
        }
        if h.is_figure() {
            self.figures.retain(|f| *f != h);
            self.tiles.remove(&h);
            if self.current_figure == Some(h) {
                self.current_figure = self.figures.last().copied();
            }
        }
        // Re-point the figure's current axes to the newest remaining one.
        if let Some(Object {
            kind: Kind::Figure(_),
            ..
        }) = self.objects.get(&o.parent)
        {
            let newest = self.objects[&o.parent]
                .children
                .iter()
                .rev()
                .find(|c| matches!(self.objects.get(c).map(|o| &o.kind), Some(Kind::Axes(_))))
                .copied();
            if let Some(Kind::Figure(f)) = self.objects.get_mut(&o.parent).map(|o| &mut o.kind)
                && f.current_axes.is_none()
            {
                f.current_axes = newest;
            }
        }
    }

    /// `close`, `close all`, `close(h)`.
    pub fn close(&mut self, target: CloseTarget) -> Result<()> {
        match target {
            CloseTarget::Current => {
                if let Some(f) = self.current_figure {
                    self.delete_tree(f);
                }
            }
            CloseTarget::All => {
                for f in self.figures.clone() {
                    self.delete_tree(f);
                }
            }
            CloseTarget::Handle(h) => {
                if !h.is_figure() || !self.objects.contains_key(&h) {
                    return Err(GraphicsError::invalid_handle());
                }
                self.delete_tree(h);
            }
        }
        Ok(())
    }

    /// `clf`: delete every child of the figure.
    pub fn clf(&mut self, fig: Option<Handle>) -> Result<()> {
        let fig = match fig {
            Some(h) => {
                self.figure_ref(h)?;
                h
            }
            None => self.gcf(),
        };
        for c in self.obj(fig)?.children.clone() {
            self.delete_tree(c);
        }
        self.tiles.remove(&fig);
        let f = self.figure_mut(fig)?;
        f.current_axes = None;
        f.hold_add = false;
        Ok(())
    }

    /// `subplot(m, n, p)`; `cells` holds one or more one-based tile numbers
    /// (`subplot(2,2,[1 2])` spans). Reuses an axes already at that slot,
    /// deletes other axes that overlap the new one.
    pub fn subplot(&mut self, rows: usize, cols: usize, cells: &[usize]) -> Result<Handle> {
        if rows == 0
            || cols == 0
            || cells.is_empty()
            || cells.iter().any(|&p| p == 0 || p > rows * cols)
        {
            return Err(GraphicsError::new(
                "MATLAB:subplot:InvalidIndex",
                "subplot: index exceeds number of subplots",
            ));
        }
        let mut cells = cells.to_vec();
        cells.sort_unstable();
        cells.dedup();
        let slot = GridSlot { rows, cols, cells };
        let fig = self.gcf();
        // The gaps are measured in pixels: use the figure's own size.
        let size = self
            .figure_ref(fig)
            .map_or(layout::FIGURE_SIZE, |f| [f.position[2], f.position[3]]);
        let position = layout::subplot_position_sized(&slot, size);
        let axes: Vec<Handle> = self.obj(fig)?.children.clone();
        for h in axes {
            let Ok(a) = self.axes_ref(h) else { continue };
            if a.slot.as_ref() == Some(&slot) || a.base_position == position {
                self.make_current(h)?;
                return Ok(h);
            }
        }
        // MATLAB/Octave delete axes the new subplot would overlap.
        for h in self.obj(fig)?.children.clone() {
            if let Ok(a) = self.axes_ref(h)
                && layout::overlaps(&a.base_position, &position)
            {
                self.delete_tree(h);
            }
        }
        let ax = self.new_axes(fig, Some(position));
        self.axes_mut(ax)?.slot = Some(slot);
        Ok(ax)
    }

    /// `tiledlayout(m, n)`: clears the figure and starts a tile grid.
    pub fn tiledlayout(&mut self, rows: usize, cols: usize) -> Result<Handle> {
        if rows == 0 || cols == 0 {
            return Err(GraphicsError::args(
                "tiledlayout",
                "grid size must be positive",
            ));
        }
        let fig = self.gcf();
        self.clf(Some(fig))?;
        self.tiles.insert(fig, (rows, cols, 0));
        Ok(fig)
    }

    /// `nexttile` / `nexttile(k)`.
    pub fn nexttile(&mut self, tile: Option<usize>) -> Result<Handle> {
        let fig = self.gcf();
        let (rows, cols, next) = match self.tiles.get(&fig) {
            Some(t) => *t,
            None => {
                // MATLAB creates a flow layout; a 1×1 grid is our nearest form.
                self.tiles.insert(fig, (1, 1, 0));
                (1, 1, 0)
            }
        };
        let k = match tile {
            Some(k) if k >= 1 && k <= rows * cols => k,
            Some(_) => {
                return Err(GraphicsError::args("nexttile", "tile number out of range"));
            }
            None => next % (rows * cols) + 1,
        };
        self.tiles.insert(fig, (rows, cols, k));
        let slot = GridSlot {
            rows,
            cols,
            cells: vec![k],
        };
        for h in self.obj(fig)?.children.clone() {
            if let Ok(a) = self.axes_ref(h)
                && a.slot.as_ref() == Some(&slot)
            {
                self.make_current(h)?;
                return Ok(h);
            }
        }
        let position = layout::tile_position(rows, cols, k);
        let ax = self.new_axes(fig, Some(position));
        self.axes_mut(ax)?.slot = Some(slot);
        Ok(ax)
    }

    /// Text object for a label/title of an axes.
    pub(crate) fn label_handle(&self, ax: Handle, role: LabelRole) -> Result<Handle> {
        let a = self.axes_ref(ax)?;
        Ok(match role {
            LabelRole::XLabel => a.x.label,
            LabelRole::YLabel => a.y.label,
            LabelRole::ZLabel => a.z.label,
            LabelRole::Title => a.title,
            LabelRole::Free | LabelRole::SuperTitle => {
                return Err(GraphicsError::args("label", "not a label role"));
            }
        })
    }

    pub(crate) fn text_mut(&mut self, h: Handle) -> Result<&mut Text> {
        match &mut self.obj_mut(h)?.kind {
            Kind::Text(t) => Ok(t),
            _ => Err(GraphicsError::invalid_handle()),
        }
    }
}
