//! Control toolbox (`builtins/control`). Expected numbers were measured
//! with GNU Octave 11 + control 4.2.3 (`octave-cli`, black-box oracle)
//! unless marked as analytic/MATLAB-documented. Tolerances are relative
//! (`tol * (1 + |want|)`).

use indy_interp::Interp;

fn run(src: &str) -> Interp {
    let mut it = Interp::new();
    if let Err(e) = it.run_text(src) {
        panic!("{}\n  -> {} ({})", src, e.message, e.identifier);
    }
    it
}

fn nums(it: &Interp, name: &str) -> Vec<f64> {
    it.get_var(name)
        .unwrap_or_else(|| panic!("no variable {}", name))
        .to_f64_vec()
        .unwrap()
}

fn check(src: &str, var: &str, want: &[f64], tol: f64) {
    let it = run(src);
    let got = nums(&it, var);
    let ok = got.len() == want.len()
        && got
            .iter()
            .zip(want)
            .all(|(x, y)| (x - y).abs() <= tol * (1.0 + y.abs()));
    assert!(ok, "{}: {} = {:?}, want {:?}", src, var, got, want);
}

fn fails(src: &str) -> (String, String) {
    let mut it = Interp::new();
    match it.run_text(src) {
        Ok(_) => panic!("expected an error: {}", src),
        Err(e) => (e.identifier.clone(), e.message.clone()),
    }
}

fn output(src: &str) -> String {
    let mut it = Interp::new();
    it.capture = Some(String::new());
    if let Err(e) = it.run_text(src) {
        panic!("{}\n  -> {}", src, e.message);
    }
    it.capture.take().unwrap()
}

const W: &str = "s = tf('s'); W = 10/(s^2+2*s+10);";

#[test]
fn time_responses_match_oracle() {
    check(
        &format!("{W} y = step(W, 0:0.5:3);"),
        "y",
        &[
            0.0,
            0.7554252876,
            1.346892837,
            1.119740384,
            0.8826600075,
            0.9458811981,
            1.038523114,
        ],
        1e-9,
    );
    check(
        &format!("{W} y = impulse(W, 0:0.5:3);"),
        "y",
        &[
            0.0,
            2.016704308,
            0.173050499,
            -0.727054839,
            -0.126049252,
            0.2566524227,
            0.068393904,
        ],
        1e-9,
    );
    // Smooth input: first-order hold, as Octave's default.
    check(
        "H = tf([1 3],[1 3 2]); y = lsim(H, sin(0:0.1:2)', 0:0.1:2); y = y(end-3:end);",
        "y",
        &[0.8733549999, 0.9251328473, 0.9706966024, 1.009326276],
        1e-9,
    );
    check(
        "y = initial(ss([0 1;-2 -3],[0;1],[1 0],0), [1;-1], 0:0.5:2);",
        "y",
        &[1.0, 0.6065306597, 0.3678794412, 0.2231301601, 0.1353352832],
        1e-9,
    );
    check(
        "y = step(ss(c2d(ss([0 1;-2 -3],[0;1],[1 0],0),0.2)), 0:0.2:1);",
        "y",
        &[
            0.0,
            0.01642926994,
            0.05434443602,
            0.1017854699,
            0.1516192949,
            0.1997882004,
        ],
        1e-9,
    );
    // Discrete impulse: unit-area pulse (MATLAB), height 1/Ts.
    check(
        "y = impulse(c2d(tf(1,[1 1]),0.2), 0:0.2:1);",
        "y",
        &[
            0.0,
            0.9063462346,
            0.7420535352,
            0.6075420497,
            0.4974133599,
            0.4072476147,
        ],
        1e-9,
    );
}

#[test]
fn lsim_is_exact_for_free_oscillation() {
    // Analytic solution of x'' + 0.8 x' + 4 x = 0, x(0) = 1.
    check(
        "t=(0:0.01:10)'; y=lsim(ss([0 1;-4 -0.8],[0;1],[1 0],0), zeros(size(t)), t, [1;0]); \
         a=0.4; wd=sqrt(4-a^2); e=max(abs(y-exp(-a*t).*(cos(wd*t)+a/wd*sin(wd*t))));",
        "e",
        &[0.0],
        1e-13,
    );
}

#[test]
fn discretisation_matches_oracle() {
    let cases: [(&str, [f64; 6]); 6] = [
        (
            "",
            [
                0.0,
                0.04644321757,
                0.04343913116,
                1.0,
                -1.728848404,
                0.8187307531,
            ],
        ),
        (
            ", 'foh'",
            [
                0.01578776229,
                0.05981137361,
                0.01428321282,
                1.0,
                -1.728848404,
                0.8187307531,
            ],
        ),
        (
            ", 'tustin'",
            [
                0.02222222222,
                0.04444444444,
                0.02222222222,
                1.0,
                -1.733333333,
                0.8222222222,
            ],
        ),
        (
            ", 'prewarp', 3",
            [
                0.02253708547,
                0.04507417094,
                0.02253708547,
                1.0,
                -1.730909232,
                0.8210575741,
            ],
        ),
        (
            ", 'matched'",
            [
                0.0,
                0.04494117436,
                0.04494117436,
                1.0,
                -1.728848404,
                0.8187307531,
            ],
        ),
        (
            ", 'impulse'",
            [0.0, 0.08913258026, 0.0, 1.0, -1.728848404, 0.8187307531],
        ),
    ];
    for (m, want) in cases {
        check(
            &format!("{W} [n,d] = tfdata(c2d(W, 0.1{m}),'v'); r = [n d];"),
            "r",
            &want,
            1e-9,
        );
    }
    check(
        &format!("{W} [n,d] = tfdata(d2c(c2d(W, 0.1)),'v'); r = [n d];"),
        "r",
        &[0.0, 0.0, 10.0, 1.0, 2.0, 10.0],
        1e-9,
    );
    check(
        &format!("{W} [n,d] = tfdata(d2c(c2d(W, 0.1,'tustin'),'tustin'),'v'); r = [n d];"),
        "r",
        &[0.0, 0.0, 10.0, 1.0, 2.0, 10.0],
        1e-9,
    );
}

#[test]
fn frequency_analysis_matches_oracle() {
    check(
        &format!("{W} [m,p] = bode(W, [0.1 1 3 10]); r = [m(:)' p(:)'];"),
        "r",
        &[
            1.00080046,
            1.084652289,
            1.643989873,
            0.1084652289,
            -1.146909442,
            -12.52880771,
            -80.53767779,
            -167.4711923,
        ],
        1e-9,
    );
    check(
        "[a,b,c,d] = margin(tf(1,[1 2 1 0])); r = [a b c d];",
        "r",
        &[2.0, 21.38638975, 1.0, 0.6823278038],
        1e-8,
    );
    check(
        "[a,b,c,d] = margin(tf(50,[1 9 30 40])); r = [a b c d];",
        "r",
        &[4.6, 100.6620302, 5.477225575, 1.84839961],
        1e-8,
    );
    check(
        "[a,b,c,d] = margin(c2d(tf(1,[1 2 1 0]),0.1)); r = [a b c d];",
        "r",
        &[1.82081307, 19.43821990, 0.953429511, 0.6822468683],
        1e-8,
    );
    // Analytic: |W(jw)| = 10^(-3/20) |W(0)|.
    check(
        &format!("{W} b = bandwidth(W); g = abs(freqresp(W, b));"),
        "g",
        &[10f64.powf(-3.0 / 20.0)],
        1e-10,
    );
}

#[test]
fn design_matches_oracle() {
    let a = "A = [0 1; -2 -3]; B = [0;1];";
    check(
        &format!("{a} K = lqr(A,B,diag([10 1]),0.5);"),
        "K",
        &[2.898979486, 1.098531319],
        1e-9,
    );
    check(
        &format!("{a} X = care(A,B,eye(2),1);"),
        "X",
        &[1.236067977, 0.2360679775, 0.2360679775, 0.2360679775],
        1e-9,
    );
    check(
        &format!("{a} X = lyap(A, eye(2));"),
        "X",
        &[1.0, -0.5, -0.5, 0.5],
        1e-12,
    );
    check(
        "X = dlyap([0.5 0.1;0 0.3], eye(2));",
        "X",
        &[1.353156647, 0.03878474467, 0.03878474467, 1.098901099],
        1e-9,
    );
    check(
        &format!("{a} K = place(A,B,[-4 -5]);"),
        "K",
        &[18.0, 6.0],
        1e-12,
    );
    // MATLAB-documented normalisation: monic denominator.
    check(
        "[n,d] = pade(0.5,3); r = [n d];",
        "r",
        &[-1.0, 24.0, -240.0, 960.0, 1.0, 24.0, 240.0, 960.0],
        1e-12,
    );
    check(
        "[n,d] = tfdata(minreal(tf([1 3 2],[1 6 11 6])),'v'); r = [n d];",
        "r",
        &[0.0, 1.0, 1.0, 3.0],
        1e-10,
    );
    check(
        "T = feedback(pid(2,1,1,0.1)*tf(1,[1 2 1]), 1); [n,d] = tfdata(T,'v'); r = [n d];",
        "r",
        &[0.0, 0.0, 12.0, 21.0, 10.0, 1.0, 12.0, 33.0, 31.0, 10.0],
        1e-10,
    );
}

#[test]
fn stepinfo_is_analytic_for_second_order() {
    // zeta = 1/sqrt(10): overshoot exp(-pi zeta/sqrt(1-zeta^2)), peak at pi/wd.
    let z = 1.0 / 10f64.sqrt();
    let os = 100.0 * (-std::f64::consts::PI * z / (1.0 - z * z).sqrt()).exp();
    check(
        &format!("{W} S = stepinfo(W); r = [S.Overshoot S.PeakTime];"),
        "r",
        &[os, std::f64::consts::PI / 3.0],
        1e-4,
    );
}

#[test]
fn objects_and_properties() {
    let it = run(
        "q = pid(2,3,4,.2); r = q; r.Kp = 5; set(q,'Kp',8); a = [q.Kp r.Kp]; \
         z = zpk([-2 -1],[-4 -3],2); z.Z{1}(1) = -9; b = z.Z{1}'; c = numel(properties(z)); \
         d = [isa(q,'lti') isa(q,'tf') isa(z,'zpk') isa(ss(z),'lti')]; e = size(zpk());",
    );
    assert_eq!(nums(&it, "a"), vec![8.0, 5.0]);
    assert_eq!(nums(&it, "b"), vec![-9.0, -1.0]);
    assert_eq!(nums(&it, "c"), vec![20.0]);
    assert_eq!(nums(&it, "d"), vec![1.0, 0.0, 1.0, 1.0]);
    assert_eq!(nums(&it, "e"), vec![0.0, 0.0]);
    let (id, msg) = fails("pid(true, 1)");
    assert_eq!(id, "Control:ltiobject:pidSet1");
    assert_eq!(
        msg,
        "The value of the \"Kp\" property must be a finite and real number."
    );
    let (id, _) = fails("q = zpk(); q.K = 3;");
    assert_eq!(id, "Control:ltiobject:zpkProperties4");
}

#[test]
fn display_is_matlab_like() {
    assert_eq!(
        output("G = tf(1,[1 1])"),
        "G =\n \n    1\n  -----\n  s + 1\n \nContinuous-time transfer function.\n\n"
    );
    let d = output("Gd = c2d(tf(1,[1 1]), 0.1)");
    assert!(
        d.contains("   0.09516\n  ----------\n  z - 0.9048\n"),
        "{}",
        d
    );
    assert!(
        d.contains("Sample time: 0.1 seconds\nDiscrete-time transfer function."),
        "{}",
        d
    );
}

#[test]
fn pkg_reports_native_control() {
    let out = output("pkg load control; pkg list");
    assert!(out.contains("control*"), "{}", out);
    let (id, _) = fails("pkg load nosuchpackage");
    assert_eq!(id, "Octave:pkg:notSupported");
}

#[test]
fn unsupported_forms_say_so() {
    let (id, _) = fails("tf({1,2},{[1 1],[1 2]})");
    assert_eq!(id, "Control:general:notSupported");
    let (id, _) = fails("G = tf(1,[1 1]); G .* G");
    assert_eq!(id, "Control:general:notSupported");
}
