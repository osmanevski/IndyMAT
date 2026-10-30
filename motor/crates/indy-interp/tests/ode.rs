//! ode45/ode23 step control and output (MATLAB ODE suite rules).

use indy_interp::Interp;

fn ok(code: &str) {
    let mut it = Interp::new();
    if let Err(e) = it.run_text(code) {
        panic!("snippet failed: {}\n---\n{}", e.message, code);
    }
}

const OSC: &str = "w = 2*pi; z = 0.12; f = @(t,x) [x(2); -2*z*w*x(2) - w^2*x(1)];
    wd = w*sqrt(1 - z^2); ex = @(t) exp(-z*w*t).*(cos(wd*t) + z/sqrt(1 - z^2)*sin(wd*t));";

#[test]
fn ode45_initial_step_and_refine() {
    ok(&format!(
        "{OSC}
         sol = ode45(f, [0 6], [1; 0]);
         % initial step: 0.8*RelTol^(1/5) / norm(f0 ./ max(|y0|, AbsTol/RelTol), inf)
         h0 = 0.8 * 1e-3^(1/5) / (w^2 / 1e-3);
         assert(abs(sol.x(2) - h0) < 1e-12 * h0);
         [t, x] = ode45(f, [0 6], [1; 0]);
         % Refine 4: three interpolated points per step
         assert(numel(t) == 4 * sol.stats.nsteps + 1 && t(end) == 6);
         assert(abs(t(2) - h0 / 4) < 1e-12 * h0);
         assert(all(diff(t) > 0) && max(abs(x(:,1) - ex(t))) < 2e-3);"
    ));
}

#[test]
fn ode45_options_tspan_and_ode23() {
    ok(&format!(
        "{OSC}
         [t, x] = ode45(f, [0 6], [1; 0], odeset('InitialStep', 0.01, 'Refine', 1));
         assert(abs(t(2) - 0.01) < 1e-15 && max(abs(x(:,1) - ex(t))) < 2e-3);
         [t, x] = ode45(f, linspace(0, 6, 13), [1; 0]);
         assert(numel(t) == 13 && isequal(x(1,:), [1 0]) && max(abs(x(:,1) - ex(t))) < 1e-3);
         [t, x] = ode45(f, [0 6], [1; 0], odeset('RelTol', 1e-8, 'AbsTol', 1e-10));
         assert(max(abs(x(:,1) - ex(t))) < 1e-6);
         [t, x] = ode23(f, [0 6], [1; 0]);
         assert(t(end) == 6 && max(abs(x(:,1) - ex(t))) < 1e-2);
         [t, y] = ode45(@(t, y) -y, [1 0], exp(-1));
         assert(t(end) == 0 && abs(y(end) - 1) < 1e-4);
         sol = ode45(@(t, y) -y, [0 1], 1); v = deval(sol, 0.5);
         assert(abs(v - exp(-0.5)) < 1e-4);"
    ));
}
