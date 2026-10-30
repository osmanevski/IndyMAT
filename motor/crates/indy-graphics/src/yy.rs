//! `yyaxis left|right`: two y rulers over one x axis.
//!
//! Modelled as IndyMAT's own Octave `yyaxis` (`octave/compat/yyaxis.m`)
//! does: a second, transparent axes at the same position with
//! `YAxisLocation 'right'` and no x tick labels; `yyaxis` switches the
//! current axes between the two, so later `plot`, `ylabel`, `ylim`, `hold`
//! act on that side. As in MATLAB, every series on a side takes the side's
//! colour (colour order entry 1 on the left, 2 on the right); MATLAB's
//! line-style cycling within one side is not modelled. `gca` is the side's
//! own handle (MATLAB keeps one axes object).
//!
//! Contract: Figure-Data has no right-hand y ruler, so a figure holding a
//! `yyaxis` pair is the explicit fallback `unsupported_object` with
//! `type: "yyaxis"` (drawing both sides on one left ruler would mislabel
//! the right-hand data).

use crate::color::COLOR_ORDER;
use crate::error::{GraphicsError, Result};
use crate::graphics::Graphics;
use crate::model::{Handle, Mode, Paint};

impl Graphics {
    /// `yyaxis(ax, side)`: returns the axes of that side, made current.
    pub fn yyaxis(&mut self, ax: Option<Handle>, right: bool) -> Result<Handle> {
        let ax = match ax {
            Some(h) => {
                self.axes_ref(h)?;
                h
            }
            None => self.gca(),
        };
        if !self.axes_ref(ax)?.is_2d_view() {
            return Err(GraphicsError::args("yyaxis", "only 2-D axes are supported"));
        }
        let partner = self
            .axes_ref(ax)?
            .yy_pair
            .filter(|p| self.axes_ref(*p).is_ok());
        let (left, right_ax) = match partner {
            Some(p) if self.axes_ref(ax)?.y_right => (p, ax),
            Some(p) => (ax, p),
            None => {
                let fig = self.ancestor_figure(ax)?;
                let (position, order) = {
                    let a = self.axes_ref(ax)?;
                    (a.position, a.color_order.clone())
                };
                self.make_current(fig)?;
                let r = self.axes(Some(position))?;
                let side = |k: usize| vec![order.get(k).copied().unwrap_or(COLOR_ORDER[k])];
                {
                    let a = self.axes_mut(r)?;
                    a.y_right = true;
                    a.color = Paint::None;
                    a.x.ticklabel = Vec::new();
                    a.x.ticklabel_mode = Mode::Manual;
                    a.yy_pair = Some(ax);
                    a.color_order = side(1);
                }
                let a = self.axes_mut(ax)?;
                a.yy_pair = Some(r);
                a.color_order = side(0);
                (ax, r)
            }
        };
        let target = if right { right_ax } else { left };
        self.make_current(target)?;
        Ok(target)
    }

    /// Whether an axes is one side of a live `yyaxis` pair.
    pub(crate) fn is_yyaxis(&self, ax: Handle) -> bool {
        self.axes_ref(ax)
            .ok()
            .and_then(|a| a.yy_pair)
            .is_some_and(|p| self.axes_ref(p).is_ok())
    }
}
