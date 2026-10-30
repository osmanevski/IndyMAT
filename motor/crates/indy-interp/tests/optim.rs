//! Optimisation and multi-dimensional integration builtins (fsolve,
//! lsqnonneg, optimoptions, fminsearch, fminunc, lsqnonlin, lsqcurvefit,
//! integral2, integral3, quad2d). Reference values are analytic, from
//! `octave-cli` (marked "Octave"), from the MATLAB documentation examples
//! (marked "MATLAB doc"), or from IndyMAT's MIT `octave/compat/fminsearch.m`
//! (MATLAB's Nelder–Mead) run under `octave-cli` (marked "compat").

use indy_interp::Interp;

fn run(src: &str) {
    let mut it = Interp::new();
    if let Err(e) = it.run_text(src) {
        panic!("{}\n  -> {} ({})", src, e.message, e.identifier);
    }
}

fn fails(src: &str, needle: &str) {
    let mut it = Interp::new();
    match it.run_text(src) {
        Ok(_) => panic!("expected an error from: {}", src),
        Err(e) => assert!(
            e.message.contains(needle),
            "{}\n  -> unexpected message: {}",
            src,
            e.message
        ),
    }
}

#[test]
fn scoreboard_cases() {
    run(r#"ayar = optimset('Display', 'off');
        x = fsolve(@(v) [v(1) + v(2) - 3; v(1) - v(2) - 1], [0; 0], ayar);
        assert(norm(x - [2; 1]) < 1e-6);
        x = lsqnonneg([1 0; 0 1], [1; -1]);
        assert(norm(x - [1; 0]) < 1e-10);
        assert(abs(integral(@(x) x.^2, 0, 1) - 1 / 3) < 1e-10);
        assert(abs(quadgk(@(x) exp(-x.^2), -Inf, Inf) - sqrt(pi)) < 1e-8);
        assert(abs(integral2(@(x, y) x .* y, 0, 1, 0, 2) - 1) < 1e-8);"#);
}

#[test]
fn fsolve_nonlinear() {
    // Octave: x = [1.414213562374707; 1.414213562374707], exitflag 1
    run(r#"o = optimset('Display', 'off');
        [x, fv, ef, out, J] = fsolve(@(x)[x(1)^2+x(2)^2-4; x(1)-x(2)], [1;1], o);
        assert(norm(x - sqrt(2)*[1;1]) < 1e-8);
        assert(ef == 1 && norm(fv) < 1e-8);
        assert(isequal(size(J), [2 2]) && norm(J - [2*sqrt(2) 2*sqrt(2); 1 -1]) < 1e-5);
        assert(strcmp(out.algorithm, 'trust-region-dogleg'));
        assert(out.iterations >= 1 && out.funcCount > out.iterations);
        assert(out.firstorderopt <= 1e-6);
        assert(all(isfield(out, {'iterations','funcCount','algorithm','firstorderopt','message'})));
        o2 = optimset(o, 'Algorithm', 'levenberg-marquardt');
        [x, ~, ef, out] = fsolve(@(x)[x(1)^2+x(2)^2-4; x(1)-x(2)], [1;1], o2);
        assert(norm(x - sqrt(2)*[1;1]) < 1e-6 && ef > 0);
        assert(strcmp(out.algorithm, 'levenberg-marquardt'));
        % shape of x0 is kept; fun receives that shape
        x = fsolve(@(v) v - [1 2; 3 4], zeros(2), o);
        assert(isequal(size(x), [2 2]) && norm(x - [1 2; 3 4]) < 1e-8);
        % scalar equation; MATLAB doc example of fsolve: x = fsolve(@(x) x - cos(x), 0)
        x = fsolve(@(x) x - cos(x), 0, o);
        assert(abs(x - 0.739085133215161) < 1e-8);
        % MATLAB doc example (iteration display): 10 iterations, 33 evaluations,
        % first-order optimality 6.36e-07, x = [0.5671; 0.5671]
        F = @(x)[2*x(1) - x(2) - exp(-x(1)); -x(1) + 2*x(2) - exp(-x(2))];
        [x, fv, ef, out] = fsolve(F, [-5; -5], o);
        assert(out.iterations == 10 && out.funcCount == 33 && ef == 1);
        assert(abs(out.firstorderopt - 6.36e-07) < 5e-9);
        assert(norm(x - 0.567143290409784*[1; 1]) < 1e-6);
        % no real root: exitflag -2 (x^2 + 1 = 0)
        [~, ~, ef] = fsolve(@(x) x^2 + 1, 3, o);
        assert(ef == -2);"#);
}

#[test]
fn fsolve_jacobian_and_options() {
    run(
        r#"o = optimoptions('fsolve', 'Display', 'off', 'SpecifyObjectiveGradient', true);
        [x, ~, ef, out] = fsolve(@jf, [0.5; 0.5], o);
        assert(norm(x - [1; 1]) < 1e-8 && ef == 1);
        % with a user Jacobian there is one evaluation per iteration
        assert(out.funcCount == out.iterations + 1);
        o = optimset('Display', 'off', 'Jacobian', 'on');
        x = fsolve(@jf, [2; 0], o);
        assert(norm(x - [1; 1]) < 1e-8);
        % overdetermined consistent system: Levenberg-Marquardt (with warning)
        warning('off', 'all');
        [x, ~, ef] = fsolve(@(x) [x(1)-1; x(2)-2; x(1)+x(2)-3], [0 0], optimset('Display','off'));
        assert(norm(x - [1 2]) < 1e-6 && ef > 0);
        % iteration limit
        [~, ~, ef, out] = fsolve(@(x)[x(1)^2+x(2)^2-4; x(1)-x(2)], [10;-3], optimset('Display','off','MaxIter',1));
        assert(ef == 0 && out.iterations == 1);
        % central differences
        x = fsolve(@(x)[x(1)^2+x(2)^2-4; x(1)-x(2)], [1;1], optimoptions('fsolve','Display','off','FiniteDifferenceType','central'));
        assert(norm(x - sqrt(2)*[1;1]) < 1e-8);
        function [F, J] = jf(x)
            F = [x(1)^3 - x(2); x(1) + x(2) - 2];
            J = [3*x(1)^2, -1; 1, 1];
        end"#,
    );
    fails(
        "fsolve(@(x) x, 1, optimset('OutputFcn', @(varargin) false))",
        "OutputFcn",
    );
    fails(
        "fsolve(@(x) x, 1, optimset('Algorithm', 'bogus', 'Display', 'off'))",
        "bogus",
    );
    fails("fsolve(@(x) [x; NaN], [1; 2])", "undefined values");
}

#[test]
fn fsolve_display() {
    run(r#"s = evalc('x = fsolve(@(x) x - 2, 0);');
        assert(~isempty(strfind(s, 'Equation solved.')));
        s = evalc('x = fsolve(@(x) x - 2, 0, optimset(''Display'', ''off''));');
        assert(isempty(s));
        s = evalc('x = fsolve(@(x) x - 2, 0, optimset(''Display'', ''iter''));');
        assert(~isempty(strfind(s, 'Iteration')) && ~isempty(strfind(s, 'Equation solved.')));
        s = evalc('x = fsolve(@(x) x - 2, 0, optimset(''Display'', ''notify''));');
        assert(isempty(s));"#);
}

#[test]
fn lsqnonneg_cases() {
    // Octave: x = [0; 0; 1.32364], resnorm 0.1670885028, lambda(1:2) = [-0.2285333568; -0.1494919236]
    run(
        r#"C = [0.0372 0.2869 0.4; 0.6861 0.7071 0.3; 0.6233 0.6245 0.1; 0.6344 0.6170 0.7];
        d = [0.8587; 0.1781; 0.0747; 0.8405];
        [x, resnorm, residual, ef, out, lambda] = lsqnonneg(C, d);
        assert(norm(x - [0; 0; 1.32364]) < 1e-10);
        assert(abs(resnorm - 0.1670885028) < 1e-9);
        assert(norm(residual - (d - C*x)) < 1e-14);
        assert(ef == 1 && strcmp(out.algorithm, 'active-set'));
        assert(norm(lambda(1:2) - [-0.2285333568; -0.1494919236]) < 1e-9 && abs(lambda(3)) < 1e-12);
        % MATLAB doc example: x = [0; 0.6929]
        C2 = C(:, 1:2);
        [x, rn] = lsqnonneg(C2, d);
        assert(norm(x - [0; 0.692934397130293]) < 1e-12 && abs(rn - 0.831455951263312) < 1e-12);
        % Octave: lsqnonneg(magic(4), [1;2;3;4]) = [0.047058823529412; 0.194117647058823; 0.052941176470589; 0]
        x = lsqnonneg(magic(4), [1;2;3;4]);
        assert(norm(x - [0.047058823529412; 0.194117647058823; 0.052941176470589; 0]) < 1e-12);
        x = lsqnonneg(C, d, optimset('Display', 'off', 'TolX', 1e-8));
        assert(norm(x - [0; 0; 1.32364]) < 1e-10);
        x = lsqnonneg(C, d, optimoptions('lsqnonneg', 'Display', 'off'));
        assert(norm(x - [0; 0; 1.32364]) < 1e-10);
        x = lsqnonneg(zeros(0, 2), zeros(0, 1));
        assert(isequal(x, [0; 0]));"#,
    );
    fails("lsqnonneg([1 2; 3 4], [1; 2; 3])", "rows");
    fails("lsqnonneg([1i 2; 3 4], [1; 2])", "real");
}

#[test]
fn optimoptions_struct() {
    run(
        r#"o = optimoptions('fsolve', 'Display', 'iter', 'TolFun', 1e-9, 'MaxIter', 50);
        assert(strcmp(o.SolverName, 'fsolve') && strcmp(o.Display, 'iter'));
        assert(o.FunctionTolerance == 1e-9 && o.MaxIterations == 50);
        assert(strcmp(o.Algorithm, 'trust-region-dogleg'));
        o2 = optimoptions(o, 'Algorithm', 'levenberg-marquardt');
        assert(strcmp(o2.Algorithm, 'levenberg-marquardt') && o2.MaxIterations == 50);
        o3 = optimoptions(@fminunc, 'SpecifyObjectiveGradient', 'on');
        assert(o3.SpecifyObjectiveGradient == true && strcmp(o3.SolverName, 'fminunc'));
        o4 = optimoptions('lsqnonlin', optimset('TolX', 1e-3));
        assert(o4.StepTolerance == 1e-3);"#,
    );
    fails("optimoptions('fsolve', 'NoSuchOption', 1)", "NoSuchOption");
    fails("optimoptions('fsolve', 'Display', 'loud')", "Display");
    fails("optimoptions('fmincon')", "fmincon");
    fails("optimoptions('fsolve', 'UseParallel', true)", "UseParallel");
}

#[test]
fn fminsearch_rosenbrock() {
    // compat (MATLAB Nelder-Mead) and MATLAB doc: fval 8.1777e-10, 85 iterations, 159 evaluations
    run(r#"f = @(x) 100*(x(2)-x(1)^2)^2 + (1-x(1))^2;
        [x, fv, ef, out] = fminsearch(f, [-1.2 1]);
        assert(isequal(size(x), [1 2]));
        assert(norm(x - [1.000022021783570 1.000042219751772]) < 1e-12);
        assert(abs(fv - 8.177661197416674e-10) < 1e-20);
        assert(ef == 1 && out.iterations == 85 && out.funcCount == 159);
        assert(strcmp(out.algorithm, 'Nelder-Mead simplex direct search'));
        % compat: 3-D quadratic from zeros, 158 iterations, 287 evaluations
        [x, fv, ef, out] = fminsearch(@(x) (x(1)-3)^2+(x(2)+1)^2+x(3)^2, [0;0;0]);
        assert(norm(x - [2.999970108872080; -0.9999939803694794; 8.477525591622098e-06]) < 1e-10);
        assert(out.iterations == 158 && out.funcCount == 287 && ef == 1);
        % extra parameters after options are passed to fun
        x = fminsearch(@(x, a) (x - a)^2, 0, [], 3);
        assert(abs(x - 3) < 1e-3);
        % limits
        s = evalc('[x, fv, ef, out] = fminsearch(f, [-1.2 1], optimset(''MaxIter'', 10));');
        assert(ef == 0 && out.iterations == 10);
        assert(~isempty(strfind(s, 'Maximum number of iterations')));
        [x, fv, ef, out] = fminsearch(f, [-1.2 1], optimset('MaxFunEvals', 30, 'Display', 'off'));
        assert(ef == 0 && out.funcCount >= 30);
        s = evalc('fminsearch(@(x) (x-2)^2, 1, optimset(''Display'', ''iter''));');
        assert(~isempty(strfind(s, 'initial simplex')) && ~isempty(strfind(s, 'Optimization terminated')));
        [x, fv, ef] = fminsearch(f, [-1.2 1], optimset('TolX', 1e-8, 'TolFun', 1e-8, 'Display', 'off'));
        assert(norm(x - [1 1]) < 1e-6 && ef == 1);"#);
    fails(
        "fminsearch(@(x) x^2, 1, optimset('OutputFcn', @(varargin) false))",
        "OutputFcn",
    );
    fails("fminsearch(@(x) [x x], 1)", "real scalar");
    fails(
        "fminsearch(@(x) NaN, 1, optimset('FunValCheck', 'on'))",
        "NaN",
    );
}

#[test]
fn fminunc_cases() {
    // analytic minima
    run(r#"o = optimoptions('fminunc', 'Display', 'off');
        [x, fv, ef, out, g, H] = fminunc(@(x) (x(1)-1)^2 + 3*(x(2)+2)^2 + x(1)*x(2), [0;0], o);
        % minimum of the quadratic: [2 1; 1 6] x = [2; -12]
        assert(norm(x - [2 1; 1 6] \ [2; -12]) < 1e-5 && ef == 1);
        assert(norm(H - [2 1; 1 6]) < 0.1 && norm(g) < 1e-5);
        assert(strcmp(out.algorithm, 'quasi-newton'));
        [x, fv, ef] = fminunc(@(x) 100*(x(2)-x(1)^2)^2 + (1-x(1))^2, [-1.2 1], o);
        assert(norm(x - [1 1]) < 1e-3 && ef > 0);
        o = optimoptions('fminunc', 'Display', 'off', 'SpecifyObjectiveGradient', true);
        [x, fv, ef] = fminunc(@rg, [-1.2; 1], o);
        assert(norm(x - [1; 1]) < 1e-4 && ef > 0);
        s = evalc('fminunc(@(x) (x-3)^2, 0);');
        assert(~isempty(strfind(s, 'Local minimum found.')));
        function [f, g] = rg(x)
            f = 100*(x(2)-x(1)^2)^2 + (1-x(1))^2;
            g = [-400*(x(2)-x(1)^2)*x(1) - 2*(1-x(1)); 200*(x(2)-x(1)^2)];
        end"#);
    fails(
        "fminunc(@(x) x^2, 1, optimoptions('fminunc', 'Algorithm', 'trust-region'))",
        "Algorithm",
    );
}

#[test]
fn lsqnonlin_lsqcurvefit() {
    run(r#"o = optimoptions('lsqnonlin', 'Display', 'off');
        % exponential fit to exact data: y = 2 exp(-1.5 t)
        t = (0:0.25:3)'; y = 2*exp(-1.5*t);
        [x, resnorm, res, ef, out] = lsqnonlin(@(p) p(1)*exp(p(2)*t) - y, [1; -1], [], [], o);
        assert(norm(x - [2; -1.5]) < 1e-6 && resnorm < 1e-12 && ef > 0);
        assert(isequal(size(res), size(t)));
        % bounds: best p(2) >= -1 with p(1) free
        [x, ~, ~, ef] = lsqnonlin(@(p) p(1)*exp(p(2)*t) - y, [1; -0.5], [-Inf; -1], [Inf; 0], o);
        assert(abs(x(2) + 1) < 1e-10 && ef > 0);
        oc = optimoptions('lsqcurvefit', 'Display', 'off');
        [x, resnorm] = lsqcurvefit(@(p, xd) p(1)*exp(p(2)*xd), [1; -1], t, y, [], [], oc);
        assert(norm(x - [2; -1.5]) < 1e-6 && resnorm < 1e-12);
        % linear least squares with residual: line through (0,1),(1,2),(2,2)
        [x, resnorm] = lsqcurvefit(@(p, xd) p(1) + p(2)*xd, [0 0], [0 1 2], [1 2 2], [], [], oc);
        assert(norm(x - [7/6 1/2]) < 1e-6 && abs(resnorm - 1/6) < 1e-8);"#);
}

#[test]
fn integral2_cases() {
    // analytic and Octave values
    run(
        r#"assert(abs(integral2(@(x,y) x.*y, 0, 1, 0, @(x) x) - 1/8) < 1e-12);
        q = integral2(@(x,y) exp(-x.^2-y.^2), 0, 1, 0, 1);
        assert(abs(q - 0.557746285351034) < 1e-12);  % Octave
        q = integral2(@(x,y) exp(-x.^2-y.^2), 0, 1, 0, 1, 'Method', 'iterated');
        assert(abs(q - 0.557746285351034) < 1e-12);
        q = integral2(@(x,y) exp(-x.^2-y.^2), -Inf, Inf, -Inf, Inf);
        assert(abs(q - pi) < 1e-6);
        % area between y = x^2 and y = sqrt(x): 1/3
        q = integral2(@(x,y) ones(size(x)), 0, 1, @(x) x.^2, @(x) sqrt(x));
        assert(abs(q - 1/3) < 1e-10);
        % weak corner singularity: (4/3)(2 sqrt(2) - 2)
        [q, e] = integral2(@(x,y) 1./sqrt(x+y), 0, 1, 0, 1);
        assert(abs(q - 4/3*(2*sqrt(2)-2)) < 1e-6 && e >= 0);
        q = integral2(@(x,y) x.*y, 0, 1, 0, 2, 'AbsTol', 1e-12, 'RelTol', 1e-10);
        assert(abs(q - 1) < 1e-12);
        % reversed limits change sign
        assert(abs(integral2(@(x,y) x.*y, 1, 0, 0, 2) + 1) < 1e-12);
        [q, e] = quad2d(@(x,y) sin(x).*cos(y), 0, pi, 0, pi/2);
        assert(abs(q - 2) < 1e-8);
        q = quad2d(@(x,y) x + y, 0, 1, 0, @(x) x);
        assert(abs(q - 0.5) < 1e-10);"#,
    );
    fails("integral2(@(x,y) 1, 0, 1, 0, 1)", "same size");
    fails("integral2(@(x,y) x.*y, 0, 1, 0, 1, 'Bogus', 1)", "bogus");
    fails(
        "integral2(@(x,y) x.*y, 0, Inf, 0, 1, 'Method', 'tiled')",
        "finite",
    );
}

#[test]
fn integral3_cases() {
    // analytic; Octave gives 0.516666666666667 for the variable-limit case
    run(
        r#"assert(abs(integral3(@(x,y,z) x.*y.*z, 0, 1, 0, 1, 0, 1) - 1/8) < 1e-12);
        q = integral3(@(x,y,z) x + y.*z, 0, 1, 0, @(x) x, 0, @(x,y) x+y);
        assert(abs(q - 31/60) < 1e-10);
        q = integral3(@(x,y,z) x + y.*z, 0, 1, 0, @(x) x, 0, @(x,y) x+y, 'Method', 'iterated');
        assert(abs(q - 31/60) < 1e-10);
        % unit ball volume
        q = integral3(@(x,y,z) ones(size(x)), -1, 1, @(x) -sqrt(1-x.^2), @(x) sqrt(1-x.^2), ...
            @(x,y) -sqrt(max(1-x.^2-y.^2, 0)), @(x,y) sqrt(max(1-x.^2-y.^2, 0)));
        assert(abs(q - 4*pi/3) < 1e-5);
        q = integral3(@(x,y,z) exp(-x.^2-y.^2-z.^2), -Inf, Inf, -Inf, Inf, -Inf, Inf);
        assert(abs(q - pi^1.5) < 1e-5);"#,
    );
}

#[test]
fn bad_inputs_do_not_panic() {
    for src in [
        "fsolve(1, 2)",
        "fsolve(@(x) x, [])",
        "fsolve(@(x) 'a', 1)",
        "fsolve(@(x) x, 1, 5)",
        "lsqnonneg('a', 1)",
        "fminsearch(@(x) x, {1})",
        "fminunc(@(x) [1 2], 1)",
        "integral2(@(x,y) x, 0, 1, 0)",
        "integral3(@(x,y,z) x, 0, 1, 0, 1, 0, 'a')",
        "optimoptions()",
        "optimoptions(3)",
        "lsqnonlin(@(x) x, 1, 2, 1)",
        "lsqcurvefit(@(p, x) p*x, 1, [1 2], [1 2 3])",
    ] {
        let mut it = Interp::new();
        let _ = it.run_text(src);
    }
}
