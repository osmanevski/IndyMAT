//! Rust builders for the oracle cases in `tests/oracle/cases.txt`: each one
//! issues the same graphics calls as the MATLAB snippet of that name.
//! Shared by `tests/oracle.rs`, `tests/validate.rs` and `examples/grafik_dump.rs`.

#![allow(dead_code)]

use indy_graphics::{
    Array, AxisCmd, BarArgs, ColormapArg, Graphics, GridArgs, LabelRole, Levels, PlotSeries,
    PropValue, ScatterArgs, opt,
};

pub type Builder = fn(&mut Graphics);

fn s(v: &str) -> Vec<String> {
    vec![v.to_string()]
}

fn range(a: f64, step: f64, b: f64) -> Vec<f64> {
    let n = ((b - a) / step + 1e-10).floor() as usize + 1;
    (0..n).map(|k| a + k as f64 * step).collect()
}

fn linspace(a: f64, b: f64, n: usize) -> Vec<f64> {
    // Octave's linspace: a + (0:n-1)*delta with the last element exact.
    let delta = (b - a) / (n - 1) as f64;
    (0..n)
        .map(|k| if k == n - 1 { b } else { a + k as f64 * delta })
        .collect()
}

fn ok<T>(r: indy_graphics::Result<T>) -> T {
    r.expect("graphics call failed")
}

fn volcano() -> Array {
    Array::from_rows(&[
        &[0., 0., 0., 0., 0.],
        &[0., 5., 5., 5., 0.],
        &[0., 5., 0., 5., 0.],
        &[0., 5., 5., 5., 0.],
        &[0., 0., 0., 0., 0.],
    ])
}

fn bump() -> Array {
    Array::from_rows(&[
        &[0., 1., 2., 1., 0.],
        &[1., 3., 5., 3., 1.],
        &[2., 5., 9., 5., 2.],
        &[1., 3., 5., 3., 1.],
        &[0., 1., 2., 1., 0.],
    ])
}

pub fn cases() -> Vec<(&'static str, Builder)> {
    vec![
        ("line_basic", |g| {
            ok(g.plot(
                None,
                &[PlotSeries::xy(&[1., 2., 4.], &[3., 5., 4.]).with_spec("-o")],
                &[],
            ));
        }),
        ("line_multi", |g| {
            let x = range(0.0, 0.5, 3.0);
            let x2: Vec<f64> = x.iter().map(|v| v * v).collect();
            let x3: Vec<f64> = x.iter().map(|v| 2.0 * v).collect();
            ok(g.plot(
                None,
                &[
                    PlotSeries::xy(&x, &x2),
                    PlotSeries::xy(&x, &x3).with_spec("r--"),
                ],
                &[],
            ));
        }),
        ("line_matrix", |g| {
            let y = Array::from_rows(&[&[1., 2.], &[3., 4.], &[5., 7.]]);
            ok(g.plot(
                None,
                &[PlotSeries {
                    y,
                    ..Default::default()
                }],
                &[],
            ));
        }),
        ("line_nan", |g| {
            ok(g.plot(
                None,
                &[PlotSeries::xy(
                    &[1., 2., f64::NAN, 4., 5.],
                    &[1., 3., 2., 5., 4.],
                )],
                &[],
            ));
        }),
        ("line_hold", |g| {
            ok(g.plot(
                None,
                &[PlotSeries::xy(&[1., 2., 3., 4., 5.], &[2., 4., 3., 5., 1.])],
                &[],
            ));
            ok(g.hold(None, Some(true)));
            ok(g.plot(
                None,
                &[PlotSeries::xy(&[1., 2., 3., 4., 5.], &[1., 2., 3., 4., 5.]).with_spec("k:")],
                &[],
            ));
            ok(g.hold(None, Some(false)));
        }),
        ("line_labels", |g| {
            let x = range(0.0, 1.0, 10.0);
            let y: Vec<f64> = x.iter().map(|v| v * v).collect();
            ok(g.plot(None, &[PlotSeries::xy(&x, &y)], &[]));
            ok(g.label(None, LabelRole::XLabel, &s("x"), &[]));
            ok(g.label(None, LabelRole::YLabel, &s("y"), &[]));
            ok(g.label(None, LabelRole::Title, &s("Kare"), &[]));
            ok(g.grid(None, Some(true), false));
        }),
        ("line_legend", |g| {
            ok(g.plot(
                None,
                &[
                    PlotSeries::xy(&[1., 2., 3.], &[1., 2., 3.]),
                    PlotSeries::xy(&[1., 2., 3.], &[3., 2., 1.]),
                ],
                &[],
            ));
            ok(g.legend(None, Some(&["a".to_string(), "b".to_string()]), None));
        }),
        ("line_legend_location", |g| {
            ok(g.plot(None, &[PlotSeries::y(&[1., 2., 3.])], &[]));
            ok(g.legend(None, Some(&s("d")), Some("southwest")));
        }),
        ("line_limits", |g| {
            ok(g.plot(None, &[PlotSeries::y(&range(1.0, 1.0, 10.0))], &[]));
            ok(g.set_limits(None, 0, Some([0.0, 20.0])));
            ok(g.set_limits(None, 1, Some([-5.0, 15.0])));
        }),
        ("line_spec_markers", |g| {
            ok(g.plot(
                None,
                &[PlotSeries::y(&[1., 2., 3., 4.]).with_spec("g^:")],
                &[opt("LineWidth", 2.0), opt("MarkerSize", 8.0)],
            ));
        }),
        ("line_sin", |g| {
            let x = range(-3.0, 0.1, 3.0);
            let x: Vec<f64> = (0..x.len()).map(|k| -3.0 + k as f64 * 0.1).collect();
            let y: Vec<f64> = x.iter().map(|v| v.sin()).collect();
            ok(g.plot(None, &[PlotSeries::xy(&x, &y)], &[]));
        }),
        ("line_large", |g| {
            let x = linspace(0.0, 1000.0, 50);
            let y: Vec<f64> = x.iter().map(|v| v.sqrt()).collect();
            ok(g.plot(None, &[PlotSeries::xy(&x, &y)], &[]));
        }),
        ("line_small", |g| {
            ok(g.plot(
                None,
                &[PlotSeries::xy(&[0.001, 0.002, 0.004], &[1e-4, 3e-4, 2e-4])],
                &[],
            ));
        }),
        ("line_set", |g| {
            let h = ok(g.plot(None, &[PlotSeries::y(&[1., 2., 3.])], &[]));
            ok(g.set(h[0], "Color", PropValue::from([0.5, 0.5, 0.5])));
            ok(g.set(h[0], "LineStyle", "--".into()));
        }),
        ("line_object", |g| {
            ok(g.line(None, &[0., 1.], &[0., 1.], None, &[]));
        }),
        ("axis_tight", |g| {
            let x = linspace(0.0, 7.0, 30);
            let y: Vec<f64> = x.iter().map(|v| v.sin()).collect();
            ok(g.plot(None, &[PlotSeries::xy(&x, &y)], &[]));
            ok(g.axis(None, &AxisCmd::Tight));
        }),
        ("axis_equal", |g| {
            ok(g.plot(None, &[PlotSeries::xy(&[0., 3.], &[0., 1.])], &[]));
            ok(g.axis(None, &AxisCmd::Equal));
        }),
        ("axis_off", |g| {
            ok(g.plot(None, &[PlotSeries::y(&[1., 2., 3.])], &[]));
            ok(g.axis(None, &AxisCmd::Off));
        }),
        ("axis_manual", |g| {
            ok(g.plot(None, &[PlotSeries::y(&[1., 2., 3.])], &[]));
            ok(g.axis(None, &AxisCmd::Limits(vec![0., 4., 0., 10.])));
        }),
        ("plot3_basic", |g| {
            let t = range(0.0, 0.5, 5.0);
            let c: Vec<f64> = t.iter().map(|v| v.cos()).collect();
            let sn: Vec<f64> = t.iter().map(|v| v.sin()).collect();
            ok(g.plot3(None, &[PlotSeries::xyz(&c, &sn, &t)], &[]));
        }),
        ("plot3_labels", |g| {
            ok(g.plot3(
                None,
                &[PlotSeries::xyz(&[0., 1., 2.], &[0., 1., 0.], &[0., 0., 1.])],
                &[],
            ));
            ok(g.label(None, LabelRole::XLabel, &s("X"), &[]));
            ok(g.label(None, LabelRole::YLabel, &s("Y"), &[]));
            ok(g.label(None, LabelRole::ZLabel, &s("Z"), &[]));
            ok(g.grid(None, Some(true), false));
        }),
        ("scatter_basic", |g| {
            let a = ScatterArgs {
                x: vec![1., 2., 3., 4.],
                y: vec![4., 1., 3., 2.],
                ..Default::default()
            };
            ok(g.scatter(None, &a, &[]));
        }),
        ("scatter_sizes", |g| {
            let a = ScatterArgs {
                x: vec![1., 2., 3.],
                y: vec![1., 4., 9.],
                sizes: vec![10., 40., 90.],
                color: Some("r".into()),
                filled: true,
                ..Default::default()
            };
            ok(g.scatter(None, &a, &[]));
        }),
        ("scatter3_basic", |g| {
            let a = ScatterArgs {
                x: vec![1., 2., 3.],
                y: vec![3., 1., 2.],
                z: Some(vec![0., 1., 4.]),
                ..Default::default()
            };
            ok(g.scatter(None, &a, &[]));
        }),
        ("bar_basic", |g| {
            let a = BarArgs {
                y: Array::row(&[3., 5., 2., 4.]),
                ..Default::default()
            };
            ok(g.bar(None, &a, false, &[]));
        }),
        ("bar_grouped", |g| {
            let a = BarArgs {
                y: Array::from_rows(&[&[1., 2.], &[3., 4.], &[5., 6.]]),
                ..Default::default()
            };
            ok(g.bar(None, &a, false, &[]));
        }),
        ("bar_stacked", |g| {
            let a = BarArgs {
                y: Array::from_rows(&[&[1., 2.], &[3., 4.]]),
                stacked: true,
                ..Default::default()
            };
            ok(g.bar(None, &a, false, &[]));
        }),
        ("barh_basic", |g| {
            let a = BarArgs {
                y: Array::row(&[2., 4., 1.]),
                ..Default::default()
            };
            ok(g.bar(None, &a, true, &[]));
        }),
        ("stairs_basic", |g| {
            ok(g.stairs(None, &PlotSeries::y(&[1., 3., 2., 4.]), &[]));
        }),
        ("stem_basic", |g| {
            ok(g.stem(None, &PlotSeries::y(&[1., 3., 2., 4.]), false, &[]));
        }),
        ("area_basic", |g| {
            ok(g.area(None, None, &Array::row(&[1., 3., 2., 4.]), &[]));
        }),
        ("fill_basic", |g| {
            ok(g.fill(None, &[0., 1., 0.5], &[0., 0., 1.], &"r".into(), &[]));
        }),
        ("patch_basic", |g| {
            ok(g.patch(
                None,
                &[0., 2., 2., 0.],
                &[0., 0., 1., 1.],
                &PropValue::from([0., 0.5, 1.]),
                &[],
            ));
        }),
        ("surf_grid", |g| {
            // [X,Y]=meshgrid(1:4,1:3)
            let x = Array::from_rows(&[&[1., 2., 3., 4.], &[1., 2., 3., 4.], &[1., 2., 3., 4.]]);
            let y = Array::from_rows(&[&[1., 1., 1., 1.], &[2., 2., 2., 2.], &[3., 3., 3., 3.]]);
            let z = Array::from_rows(&[&[2., 3., 4., 5.], &[3., 4., 5., 6.], &[4., 5., 6., 7.]]);
            ok(g.surf(None, &GridArgs::xyz(x, y, z), &[]));
        }),
        ("surf_matrix", |g| {
            let z = Array::from_rows(&[&[1., 2., 3.], &[2., 4., 6.], &[3., 6., 9.]]);
            ok(g.surf(None, &GridArgs::z(z), &[]));
        }),
        ("mesh_basic", |g| {
            ok(g.mesh(
                None,
                &GridArgs::z(Array::from_rows(&[&[1., 2., 3.], &[4., 5., 6.]])),
                &[],
            ));
        }),
        ("surf_colorbar", |g| {
            ok(g.surf(
                None,
                &GridArgs::z(Array::from_rows(&[&[1., 2., 3.], &[4., 5., 6.]])),
                &[],
            ));
            ok(g.colorbar(None, None));
        }),
        ("surf_colormap", |g| {
            ok(g.surf(
                None,
                &GridArgs::z(Array::from_rows(&[&[1., 2.], &[3., 4.]])),
                &[],
            ));
            ok(g.colormap(None, Some(&ColormapArg::Name("jet".into(), Some(8)))));
            ok(g.caxis(None, Some([0.0, 5.0])));
        }),
        ("surf_view", |g| {
            ok(g.surf(
                None,
                &GridArgs::z(Array::from_rows(&[&[1., 2.], &[3., 4.]])),
                &[],
            ));
            ok(g.view(None, [45.0, 60.0]));
        }),
        ("contour_basic", |g| {
            let z = Array::from_rows(&[&[1., 2., 3.], &[2., 3., 4.], &[3., 4., 5.]]);
            ok(g.contour(None, &GridArgs::z(z), &Levels::Auto, false, &[]));
        }),
        ("contourf_basic", |g| {
            let z = Array::from_rows(&[&[1., 2., 3.], &[2., 3., 4.], &[3., 4., 5.]]);
            ok(g.contour(None, &GridArgs::z(z), &Levels::Auto, true, &[]));
        }),
        ("imagesc_basic", |g| {
            ok(g.image(
                None,
                None,
                None,
                Array::from_rows(&[&[1., 2.], &[3., 4.]]),
                true,
                &[],
            ));
        }),
        ("imagesc_colorbar", |g| {
            let c = Array::from_rows(&[&[8., 1., 6.], &[3., 5., 7.], &[4., 9., 2.]]);
            ok(g.image(None, None, None, c, true, &[]));
            ok(g.colorbar(None, None));
        }),
        ("imagesc_gray", |g| {
            ok(g.image(
                None,
                None,
                None,
                Array::from_rows(&[&[0., 1.], &[2., 3.]]),
                true,
                &[],
            ));
            ok(g.colormap(None, Some(&ColormapArg::Name("gray".into(), None))));
        }),
        ("imagesc_caxis", |g| {
            ok(g.image(
                None,
                None,
                None,
                Array::from_rows(&[&[1., 2.], &[3., 4.]]),
                true,
                &[],
            ));
            ok(g.caxis(None, Some([0.0, 10.0])));
        }),
        ("text_basic", |g| {
            ok(g.plot(None, &[PlotSeries::y(&[1., 2., 3.])], &[]));
            ok(g.text(None, [2.0, 2.0, 0.0], &s("merhaba"), &[]));
        }),
        ("subplot_2x1", |g| {
            ok(g.subplot(2, 1, &[1]));
            ok(g.plot(None, &[PlotSeries::y(&[1., 2., 3.])], &[]));
            ok(g.subplot(2, 1, &[2]));
            ok(g.plot(None, &[PlotSeries::y(&[3., 2., 1.])], &[]));
        }),
        ("subplot_2x2", |g| {
            for k in 1..=4usize {
                ok(g.subplot(2, 2, &[k]));
                let y: Vec<f64> = (1..=k + 1).map(|v| v as f64).collect();
                ok(g.plot(None, &[PlotSeries::y(&y)], &[]));
            }
        }),
        ("clf_replot", |g| {
            ok(g.plot(None, &[PlotSeries::y(&[1., 2., 3.])], &[]));
            ok(g.clf(None));
            ok(g.plot(None, &[PlotSeries::y(&[3., 2., 1.])], &[]));
        }),
        ("contour_ring", |g| {
            ok(g.contour(None, &GridArgs::z(bump()), &Levels::Auto, false, &[]));
        }),
        ("contourf_ring", |g| {
            ok(g.contour(None, &GridArgs::z(bump()), &Levels::Auto, true, &[]));
        }),
        ("contour_saddle_n5", |g| {
            let z = Array::from_rows(&[&[1., 0., 1.], &[0., 1., 0.], &[1., 0., 1.]]);
            ok(g.contour(None, &GridArgs::z(z), &Levels::Count(5), false, &[]));
        }),
        ("contour_xy", |g| {
            let z = Array::from_rows(&[&[1., 2., 3.], &[3., 2., 1.]]);
            let a = GridArgs::xyz(Array::row(&[0., 1., 3.]), Array::row(&[0., 2.]), z);
            ok(g.contour(None, &a, &Levels::Auto, false, &[]));
        }),
        ("contour_levels", |g| {
            ok(g.contour(
                None,
                &GridArgs::z(bump()),
                &Levels::Values(vec![2., 4., 6.]),
                false,
                &[],
            ));
        }),
        ("contourf_levels", |g| {
            ok(g.contour(
                None,
                &GridArgs::z(bump()),
                &Levels::Values(vec![2., 4., 6.]),
                true,
                &[],
            ));
        }),
        ("contourf_hole", |g| {
            ok(g.contour(
                None,
                &GridArgs::z(volcano()),
                &Levels::Values(vec![1., 3.]),
                true,
                &[],
            ));
        }),
        ("contour_hole", |g| {
            ok(g.contour(
                None,
                &GridArgs::z(volcano()),
                &Levels::Values(vec![1., 3.]),
                false,
                &[],
            ));
        }),
        ("empty_figure", |g| {
            g.gcf();
        }),
    ]
}

/// Cases with no Octave measurement: they exercise reduction, budgets,
/// alpha, log axes and every rejection path through the admission gate.
pub fn extra_cases() -> Vec<(&'static str, Builder)> {
    vec![
        ("x_many_points", |g| {
            let x: Vec<f64> = (0..5000).map(|k| k as f64 / 100.0).collect();
            let y: Vec<f64> = x.iter().map(|v| (3.0 * v).sin()).collect();
            ok(g.plot(None, &[PlotSeries::xy(&x, &y)], &[]));
        }),
        ("x_plot3_many_nan", |g| {
            let t: Vec<f64> = (0..3000).map(|k| k as f64 / 50.0).collect();
            let mut z: Vec<f64> = t.clone();
            z[1500] = f64::NAN;
            let c: Vec<f64> = t.iter().map(|v| v.cos()).collect();
            let sn: Vec<f64> = t.iter().map(|v| v.sin()).collect();
            ok(g.plot3(None, &[PlotSeries::xyz(&c, &sn, &z)], &[]));
        }),
        ("x_surf_nan_alpha", |g| {
            let z = Array::from_rows(&[&[1., 2., f64::NAN], &[4., 5., 6.], &[7., 8., 9.]]);
            let h = ok(g.surf(None, &GridArgs::z(z), &[]));
            ok(g.set(h, "FaceAlpha", 0.5.into()));
        }),
        ("x_surf_top_view", |g| {
            ok(g.surf(
                None,
                &GridArgs::z(Array::from_rows(&[&[1., 2.], &[3., 4.]])),
                &[],
            ));
            ok(g.view(None, [0.0, 90.0]));
        }),
        ("x_surf_truecolor", |g| {
            let z = Array::from_rows(&[&[1., 2.], &[3., 4.]]);
            let c = Array::new(
                &[2, 2, 3],
                vec![0., 1., 0.5, 0.2, 1., 0., 0.5, 0.3, 0., 0., 1., 1.],
            )
            .unwrap();
            ok(g.surf(
                None,
                &GridArgs {
                    c: Some(c),
                    ..GridArgs::z(z)
                },
                &[],
            ));
        }),
        ("x_mesh_interp", |g| {
            let h = ok(g.surf(
                None,
                &GridArgs::z(Array::from_rows(&[&[1., 2.], &[3., 4.]])),
                &[],
            ));
            ok(g.set(h, "FaceColor", "interp".into()));
        }),
        ("x_imagesc_rgb_uint8", |g| {
            let c = Array::new(&[1, 2, 3], vec![255., 0., 0., 255., 0., 0.])
                .unwrap()
                .with_class(indy_graphics::NumClass::UInt8);
            ok(g.image(None, None, None, c, false, &[]));
        }),
        ("x_log_axis", |g| {
            let h = ok(g.plot(
                None,
                &[PlotSeries::xy(&[1., 10., 100., 1000.], &[1., 2., 3., 4.])],
                &[],
            ));
            let ax = g.parent_of(h[0]).unwrap();
            ok(g.set(ax, "XScale", "log".into()));
        }),
        ("x_scatter_per_point", |g| {
            let a = ScatterArgs {
                x: vec![1., 2., 3., 4.],
                y: vec![1., 2., 3., 4.],
                color: Some(PropValue::Array(Array::row(&[1., 2., 3., 4.]))),
                ..Default::default()
            };
            ok(g.scatter(None, &a, &[]));
        }),
        ("x_scatter3_indexed", |g| {
            let a = ScatterArgs {
                x: vec![1., 2., 3.],
                y: vec![1., 2., 3.],
                z: Some(vec![1., 2., 3.]),
                color: Some(PropValue::Num(0.5)),
                ..Default::default()
            };
            ok(g.scatter(None, &a, &[]));
        }),
        ("x_marker_diamond", |g| {
            ok(g.plot(None, &[PlotSeries::y(&[1., 2.]).with_spec("d")], &[]));
        }),
        ("x_surface_budget", |g| {
            let z = Array::matrix(201, 200, vec![1.0; 201 * 200]);
            ok(g.surf(None, &GridArgs::z(z), &[]));
        }),
        ("x_patch_interp", |g| {
            ok(g.patch(
                None,
                &[0., 1., 0.],
                &[0., 0., 1.],
                &PropValue::Array(Array::col(&[1., 2., 3.])),
                &[],
            ));
        }),
        ("x_patch_axis_equal", |g| {
            ok(g.fill(None, &[0., 1., 0.], &[0., 0., 1.], &"b".into(), &[]));
            ok(g.axis(None, &AxisCmd::Equal));
        }),
        ("x_patch_scaled_color", |g| {
            ok(g.patch(
                None,
                &[0., 1., 0.],
                &[0., 0., 1.],
                &PropValue::Num(0.3),
                &[],
            ));
            ok(g.caxis(None, Some([0.0, 1.0])));
        }),
        ("x_text_3d", |g| {
            ok(g.plot3(
                None,
                &[PlotSeries::xyz(&[0., 1.], &[0., 1.], &[0., 1.])],
                &[],
            ));
            ok(g.text(None, [0.5, 0.5, 0.5], &s("3d"), &[]));
        }),
        ("x_text_props", |g| {
            ok(g.plot(None, &[PlotSeries::y(&[1., 2., 3.])], &[]));
            let t = ok(g.text(
                None,
                [1.5, 2.5, 0.0],
                &["two\nlines".to_string(), "\\sigma".to_string()],
                &[],
            ));
            ok(g.set(t, "Rotation", (-90.0).into()));
            ok(g.set(t, "FontWeight", "bold".into()));
            ok(g.set(t, "BackgroundColor", "y".into()));
            ok(g.set(t, "Units", "normalized".into()));
        }),
        ("x_three_figures_last", |g| {
            ok(g.plot(None, &[PlotSeries::y(&[1., 2.])], &[]));
            ok(g.figure(Some(5.0)));
            ok(g.bar(
                None,
                &BarArgs {
                    y: Array::row(&[1., -2., 3.]),
                    ..Default::default()
                },
                false,
                &[],
            ));
            ok(g.legend(None, None, Some("best")));
        }),
        ("x_subplot_colorbars", |g| {
            for k in 1..=4usize {
                ok(g.subplot(2, 2, &[k]));
                ok(g.image(
                    None,
                    None,
                    None,
                    Array::from_rows(&[&[k as f64, 2.], &[3., 9.]]),
                    true,
                    &[],
                ));
                ok(g.colorbar(
                    None,
                    Some(["eastoutside", "southoutside", "west", "north"][k - 1]),
                ));
            }
        }),
        ("x_bar_stacked_neg_barh", |g| {
            let a = BarArgs {
                x: Some(vec![1., 3., 7.]),
                y: Array::from_rows(&[&[1., -2.], &[3., 4.], &[-5., 6.]]),
                stacked: true,
                ..Default::default()
            };
            ok(g.bar(None, &a, true, &[]));
            ok(g.legend(None, Some(&["p".to_string(), "q".to_string()]), None));
        }),
        ("x_area_matrix_stairs_hold", |g| {
            ok(g.area(
                None,
                Some(&[1., 2., 3.]),
                &Array::from_rows(&[&[1., 2.], &[2., 5.], &[3., 1.]]),
                &[],
            ));
            ok(g.hold(None, Some(true)));
            ok(g.stairs(
                None,
                &PlotSeries::xy(&[1., 2., 3.], &[4., 2., 6.]).with_spec("k--"),
                &[],
            ));
            ok(g.stem(None, &PlotSeries::xy(&[1.5, 2.5], &[3., 1.]), true, &[]));
        }),
        ("x_contour_levels", |g| {
            let z = Array::from_rows(&[
                &[0., 1., 2., 1., 0.],
                &[1., 3., 5., 3., 1.],
                &[2., 5., 9., 5., 2.],
                &[1., 3., 5., 3., 1.],
                &[0., 1., 2., 1., 0.],
            ]);
            ok(g.contour(None, &GridArgs::z(z.clone()), &Levels::Count(4), false, &[]));
            ok(g.colorbar(None, None));
        }),
        ("x_contourf_ring", |g| {
            let z = Array::from_rows(&[
                &[0., 1., 2., 1., 0.],
                &[1., 3., 5., 3., 1.],
                &[2., 5., 9., 5., 2.],
                &[1., 3., 5., 3., 1.],
                &[0., 1., 2., 1., 0.],
            ]);
            ok(g.contour(
                None,
                &GridArgs::z(z),
                &Levels::Values(vec![2.0, 4.0, 6.0]),
                true,
                &[],
            ));
        }),
        ("x_contour_nan", |g| {
            let mut z = volcano();
            z.data[12] = f64::NAN;
            ok(g.contour(
                None,
                &GridArgs::z(z.clone()),
                &Levels::Values(vec![1., 3.]),
                false,
                &[],
            ));
            ok(g.figure(None));
            ok(g.contour(
                None,
                &GridArgs::z(z),
                &Levels::Values(vec![1., 3.]),
                true,
                &[],
            ));
        }),
        ("x_tiledlayout", |g| {
            ok(g.tiledlayout(1, 2));
            ok(g.nexttile(None));
            ok(g.plot(None, &[PlotSeries::y(&[1., 3., 2.])], &[]));
            ok(g.nexttile(None));
            ok(g.scatter(
                None,
                &ScatterArgs {
                    x: vec![1., 2.],
                    y: vec![2., 1.],
                    ..Default::default()
                },
                &[],
            ));
        }),
        ("x_empty_axes", |g| {
            ok(g.axes(None));
        }),
    ]
}

/// Builds a case in a fresh session and returns its figure JSON.
pub fn run(build: Builder) -> String {
    let mut g = Graphics::new();
    let fig = ok(g.figure(None));
    build(&mut g);
    let _ = fig;
    let fig = g.gcf();
    g.figure_json(fig)
}

/// Deterministic pseudo-random input generator (no crates allowed).
pub struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn pick<'a, T>(&mut self, v: &'a [T]) -> &'a T {
        &v[self.next() as usize % v.len()]
    }
    fn value(&mut self) -> f64 {
        *self.pick(&[
            0.0,
            1.0,
            -1.0,
            2.5,
            1e300,
            -1e-300,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            3.0,
            7.0,
        ])
    }
    fn vec(&mut self) -> Vec<f64> {
        let n = self.next() as usize % 6;
        (0..n).map(|_| self.value()).collect()
    }
    fn array(&mut self) -> Array {
        let r = self.next() as usize % 4;
        let c = self.next() as usize % 4;
        Array::matrix(r, c, (0..r * c).map(|_| self.value()).collect())
    }
}

/// A random session: eight random graphics calls (errors are fine, panics
/// are not). Used by the fuzz test and fed to the admission gate.
pub fn random_session(seed: u64) -> Graphics {
    let mut rng = Lcg(seed);
    let mut g = Graphics::new();
    for _ in 0..8 {
        let choice = rng.next() % 16;
        let x = rng.vec();
        let y = rng.vec();
        let _ = match choice {
            0 => g
                .plot(
                    None,
                    &[PlotSeries {
                        x: Some(rng.array()),
                        y: rng.array(),
                        ..Default::default()
                    }],
                    &[],
                )
                .map(|_| ()),
            1 => g
                .plot3(None, &[PlotSeries::xyz(&x, &y, &x)], &[])
                .map(|_| ()),
            2 => g
                .scatter(
                    None,
                    &ScatterArgs {
                        x: x.clone(),
                        y: y.clone(),
                        sizes: rng.vec(),
                        ..Default::default()
                    },
                    &[],
                )
                .map(|_| ()),
            3 => g
                .bar(
                    None,
                    &BarArgs {
                        y: rng.array(),
                        stacked: rng.next().is_multiple_of(2),
                        width: Some(rng.value()),
                        ..Default::default()
                    },
                    rng.next().is_multiple_of(2),
                    &[],
                )
                .map(|_| ()),
            4 => g.surf(None, &GridArgs::z(rng.array()), &[]).map(|_| ()),
            5 => g
                .contour(
                    None,
                    &GridArgs::z(rng.array()),
                    &Levels::Count(rng.next() as usize % 5),
                    rng.next().is_multiple_of(2),
                    &[],
                )
                .map(|_| ()),
            6 => g
                .image(None, None, None, rng.array(), true, &[])
                .map(|_| ()),
            7 => g
                .fill(None, &x, &y, &PropValue::Array(Array::row(&rng.vec())), &[])
                .map(|_| ()),
            8 => g.set_limits(
                None,
                rng.next() as usize % 3,
                Some([rng.value(), rng.value()]),
            ),
            9 => g.axis(
                None,
                rng.pick(&[
                    AxisCmd::Tight,
                    AxisCmd::Equal,
                    AxisCmd::Normal,
                    AxisCmd::Off,
                    AxisCmd::Limits(vec![0., 1., 0., 1.]),
                ]),
            ),
            10 => g
                .subplot(
                    1 + rng.next() as usize % 3,
                    1 + rng.next() as usize % 3,
                    &[1 + rng.next() as usize % 9],
                )
                .map(|_| ()),
            11 => g
                .colorbar(None, Some(rng.pick(&["eastoutside", "north", "bogus"])))
                .map(|_| ()),
            12 => g
                .text(None, [rng.value(), rng.value(), 0.0], &["t".into()], &[])
                .map(|_| ()),
            13 => g.view(None, [rng.value(), rng.value()]),
            14 => g.stairs(None, &PlotSeries::xy(&x, &y), &[]).map(|_| ()),
            _ => g.area(None, None, &rng.array(), &[]).map(|_| ()),
        };
    }
    g
}
