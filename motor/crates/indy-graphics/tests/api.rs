//! Behaviour of the object model through the public API: current-object
//! rules, hold, legends, subplots, colorbars, set/get, errors, reduction.

use indy_graphics::json::{self, Json};
use indy_graphics::{
    Array, AxisCmd, BarArgs, CloseTarget, ColormapArg, Graphics, GridArgs, Handle, LabelRole,
    Levels, PlotSeries, PropValue, ScatterArgs, opt,
};

fn record(g: &mut Graphics) -> Json {
    let f = g.gcf();
    json::parse(&g.figure_json(f)).expect("valid JSON")
}

fn nums(j: &Json, path: &str) -> Vec<f64> {
    j.at(path).and_then(Json::as_f64_vec).unwrap_or_default()
}

fn text(j: &Json, path: &str) -> String {
    j.at(path).and_then(Json::as_str).unwrap_or("").to_string()
}

#[test]
fn gcf_gca_create_on_demand_and_figure_numbers() {
    let mut g = Graphics::new();
    assert_eq!(g.current_figure(), None);
    let ax = g.gca();
    let f = g.gcf();
    assert_eq!(f, Handle(1));
    assert_eq!(g.parent_of(ax).unwrap(), f);
    assert_eq!(g.figure(None).unwrap(), Handle(2));
    assert_eq!(g.figure(Some(7.0)).unwrap(), Handle(7));
    assert_eq!(g.figure(None).unwrap(), Handle(3));
    g.make_current(f).unwrap();
    assert_eq!(g.gca(), ax, "figure 1 keeps its axes");
    g.close(CloseTarget::Handle(Handle(7))).unwrap();
    assert!(!g.is_valid(Handle(7)));
    g.close(CloseTarget::All).unwrap();
    assert!(g.figure_handles().is_empty());
    assert!(g.figure(Some(0.5)).is_err());
}

#[test]
fn hold_continues_the_colour_order_and_replot_resets() {
    let mut g = Graphics::new();
    let a = g.plot(None, &[PlotSeries::y(&[1., 2.])], &[]).unwrap();
    g.hold(None, Some(true)).unwrap();
    let b = g.plot(None, &[PlotSeries::y(&[2., 1.])], &[]).unwrap();
    assert!(g.ishold(None));
    assert_eq!(
        g.get(a[0], "Color").unwrap(),
        PropValue::row(&[0.0, 0.447, 0.741])
    );
    assert_eq!(
        g.get(b[0], "Color").unwrap(),
        PropValue::row(&[0.85, 0.325, 0.098])
    );
    g.hold(None, Some(false)).unwrap();
    g.label(None, LabelRole::Title, &["t".into()], &[]).unwrap();
    let c = g.plot(None, &[PlotSeries::y(&[3., 1.])], &[]).unwrap();
    assert!(
        !g.is_valid(a[0]) && !g.is_valid(b[0]),
        "replace deletes old children"
    );
    assert_eq!(
        g.get(c[0], "Color").unwrap(),
        PropValue::row(&[0.0, 0.447, 0.741])
    );
    let r = record(&mut g);
    assert_eq!(text(&r, "axes.0.title"), "", "replace resets the title");
}

#[test]
fn legend_labels_children_and_is_dropped_on_replot() {
    let mut g = Graphics::new();
    let h = g
        .plot(
            None,
            &[
                PlotSeries::y(&[1., 2.]),
                PlotSeries::y(&[2., 3.]).with_spec("r"),
            ],
            &[],
        )
        .unwrap();
    g.legend(
        None,
        Some(&["first".into(), "second".into()]),
        Some("NorthWest"),
    )
    .unwrap();
    assert_eq!(
        g.get(h[1], "DisplayName").unwrap(),
        PropValue::str("second")
    );
    let r = record(&mut g);
    assert_eq!(r.at("axes.0.legend.visible"), Some(&Json::Bool(true)));
    assert_eq!(text(&r, "axes.0.legend.location"), "northwest");
    assert_eq!(text(&r, "axes.0.series.1.display_name"), "second");
    assert!(g.legend(None, None, Some("middle")).is_err());
    g.plot(None, &[PlotSeries::y(&[1., 2.])], &[]).unwrap();
    let r = record(&mut g);
    assert_eq!(r.at("axes.0.legend.visible"), Some(&Json::Bool(false)));
}

#[test]
fn subplot_reuses_slots_and_replaces_overlaps() {
    let mut g = Graphics::new();
    let a = g.subplot(2, 2, &[1]).unwrap();
    let b = g.subplot(2, 2, &[2]).unwrap();
    assert_ne!(a, b);
    assert_eq!(g.subplot(2, 2, &[1]).unwrap(), a);
    let span = g.subplot(2, 2, &[1, 2]).unwrap();
    assert!(!g.is_valid(a) && !g.is_valid(b), "the span overlaps both");
    let p = g.get(span, "Position").unwrap().numbers().unwrap();
    assert!((p[2] - 0.775).abs() < 1e-12);
    assert!(g.subplot(2, 2, &[5]).is_err());
}

#[test]
fn colorbar_shrinks_its_axes_and_delete_restores() {
    let mut g = Graphics::new();
    g.surf(
        None,
        &GridArgs::z(Array::from_rows(&[&[1., 2.], &[3., 4.]])),
        &[],
    )
    .unwrap();
    let ax = g.gca();
    let cb = g.colorbar(None, None).unwrap();
    let p = g.get(ax, "Position").unwrap().numbers().unwrap();
    assert!((p[2] - 0.62).abs() < 1e-12);
    let r = record(&mut g);
    assert_eq!(nums(&r, "axes.0.colorbars.0.limits"), vec![1.0, 4.0]);
    g.delete(cb).unwrap();
    let p = g.get(ax, "Position").unwrap().numbers().unwrap();
    assert!((p[2] - 0.775).abs() < 1e-12);
}

#[test]
fn set_get_round_trip_and_errors() {
    let mut g = Graphics::new();
    let h = g
        .plot(
            None,
            &[PlotSeries::y(&[1., 2., 3.])],
            &[opt("LineWidth", 2.0)],
        )
        .unwrap()[0];
    assert_eq!(g.get(h, "linewidth").unwrap(), PropValue::Num(2.0));
    g.set(h, "Marker", "square".into()).unwrap();
    assert_eq!(g.get(h, "Marker").unwrap(), PropValue::str("s"));
    g.set(h, "Color", "m".into()).unwrap();
    assert_eq!(g.get(h, "Color").unwrap(), PropValue::row(&[1.0, 0.0, 1.0]));
    assert_eq!(g.get(h, "Type").unwrap(), PropValue::str("line"));
    assert_eq!(
        g.set(h, "Bogus", 1.0.into()).unwrap_err().id,
        "MATLAB:hg:InvalidProperty"
    );
    assert_eq!(
        g.set(h, "LineWidth", (-1.0).into()).unwrap_err().id,
        "MATLAB:hg:InvalidPropertyValue"
    );
    assert!(g.set(h, "Parent", PropValue::Handle(Handle(1))).is_err());
    let ax = g.gca();
    assert_eq!(g.get(ax, "XLim").unwrap(), PropValue::row(&[1.0, 3.0]));
    assert_eq!(g.get(ax, "XLimMode").unwrap(), PropValue::str("auto"));
    g.set(ax, "XTick", PropValue::row(&[1.0, 2.0])).unwrap();
    assert_eq!(g.get(ax, "XTickMode").unwrap(), PropValue::str("manual"));
    assert_eq!(
        g.get(ax, "XTickLabel").unwrap(),
        PropValue::Cell(vec!["1".into(), "2".into()])
    );
    assert!(
        g.plot(None, &[PlotSeries::y(&[1.]).with_spec("rr")], &[])
            .is_err()
    );
    assert!(
        g.plot(None, &[PlotSeries::xy(&[1., 2.], &[1., 2., 3.])], &[])
            .is_err()
    );
    g.delete(h).ok();
    assert!(g.get(h, "Color").is_err());
}

#[test]
fn matrix_arguments_follow_matlab_pairing() {
    let mut g = Graphics::new();
    // A length-2 X matches cols(Y), so each row of the 3x2 Y is a line.
    let y = Array::from_rows(&[&[1., 2.], &[3., 4.], &[5., 6.]]);
    let h = g
        .plot(
            None,
            &[PlotSeries {
                x: Some(Array::row(&[10., 20.])),
                y,
                ..Default::default()
            }],
            &[],
        )
        .unwrap();
    assert_eq!(h.len(), 3, "x has length cols(Y): one line per row");
    assert_eq!(g.get(h[0], "YData").unwrap(), PropValue::row(&[1.0, 2.0]));
}

#[test]
fn long_lines_are_reduced_with_source_indices() {
    let mut g = Graphics::new();
    let x: Vec<f64> = (0..5000).map(|k| k as f64).collect();
    let y: Vec<f64> = x.iter().map(|v| (v / 50.0).sin()).collect();
    g.plot(None, &[PlotSeries::xy(&x, &y)], &[]).unwrap();
    let r = record(&mut g);
    let s = r.at("axes.0.series.0").unwrap();
    assert_eq!(s.get("decimated"), Some(&Json::Bool(true)));
    assert_eq!(
        s.get("original_points").and_then(Json::as_f64),
        Some(5000.0)
    );
    let n = s.get("rendered_points").and_then(Json::as_f64).unwrap();
    assert!(n <= 2000.0 && n > 1000.0);
    let idx = nums(&r, "axes.0.series.0.source_indices");
    assert_eq!(idx.first(), Some(&0.0));
    assert_eq!(idx.last(), Some(&4999.0));
    assert_eq!(r.get("decimated"), Some(&Json::Bool(true)));
}

#[test]
fn version_rule_v2_for_plain_2d_v3_otherwise() {
    let mut g = Graphics::new();
    g.plot(None, &[PlotSeries::y(&[1., 2.])], &[]).unwrap();
    assert_eq!(
        record(&mut g).get("version").and_then(Json::as_f64),
        Some(2.0)
    );
    g.axis(None, &AxisCmd::Off).unwrap();
    assert_eq!(
        record(&mut g).get("version").and_then(Json::as_f64),
        Some(3.0)
    );
    g.axis(None, &AxisCmd::On).unwrap();
    g.bar(
        None,
        &BarArgs {
            y: Array::row(&[1., 2.]),
            ..Default::default()
        },
        false,
        &[],
    )
    .unwrap();
    assert_eq!(
        record(&mut g).get("version").and_then(Json::as_f64),
        Some(3.0)
    );
}

#[test]
fn colormap_and_caxis() {
    let mut g = Graphics::new();
    g.image(
        None,
        None,
        None,
        Array::from_rows(&[&[1., 2.], &[3., 4.]]),
        true,
        &[],
    )
    .unwrap();
    let map = g
        .colormap(None, Some(&ColormapArg::Name("hot".into(), Some(4))))
        .unwrap();
    assert_eq!(map.len(), 4);
    assert!(
        g.colormap(None, Some(&ColormapArg::Name("nope".into(), None)))
            .is_err()
    );
    g.caxis(None, Some([0.0, 8.0])).unwrap();
    let r = record(&mut g);
    assert_eq!(nums(&r, "axes.0.clim"), vec![0.0, 8.0]);
    assert_eq!(nums(&r, "axes.0.colormap.shape"), vec![4.0, 3.0]);
    assert_eq!(text(&r, "axes.0.ydir"), "reverse");
}

#[test]
fn scatter_and_contour_argument_checks() {
    let mut g = Graphics::new();
    let bad = ScatterArgs {
        x: vec![1., 2.],
        y: vec![1.],
        ..Default::default()
    };
    assert!(g.scatter(None, &bad, &[]).is_err());
    let one_row = GridArgs::z(Array::row(&[1., 2., 3.]));
    assert!(
        g.contour(None, &one_row, &Levels::Auto, false, &[])
            .is_err()
    );
    assert!(g.surf(None, &one_row, &[]).is_err());
}

mod cases;

#[test]
fn random_calls_never_panic_and_always_emit_valid_json() {
    for seed in 0..400 {
        let g = cases::random_session(seed);
        for f in g.figure_handles() {
            assert!(
                json::parse(&g.figure_json(f)).is_some(),
                "seed {seed}: invalid JSON"
            );
        }
    }
}

#[test]
fn log_axes_use_decades_and_tex_labels() {
    // Measured in Octave 11.3: semilogx([1 10 100 1000], ...) and
    // loglog([0.03 4], [5 5e4]).
    let mut g = Graphics::new();
    g.plot_log(
        None,
        &[PlotSeries::xy(&[1., 10., 100., 1000.], &[1., 2., 3., 4.])],
        true,
        false,
        &[],
    )
    .unwrap();
    let r = record(&mut g);
    assert_eq!(nums(&r, "axes.0.xlim"), vec![1.0, 1000.0]);
    assert_eq!(nums(&r, "axes.0.xtick"), vec![1.0, 10.0, 100.0, 1000.0]);
    assert_eq!(text(&r, "axes.0.xticklabel.3"), "10^{3}");
    assert_eq!(text(&r, "axes.0.xscale"), "log");
    g.plot_log(
        None,
        &[PlotSeries::xy(&[0.03, 4.], &[5., 5e4])],
        true,
        true,
        &[],
    )
    .unwrap();
    let r = record(&mut g);
    assert_eq!(nums(&r, "axes.0.xlim"), vec![0.01, 10.0]);
    assert_eq!(nums(&r, "axes.0.ylim"), vec![1.0, 100000.0]);
    assert_eq!(nums(&r, "axes.0.ytick").len(), 6);
}
