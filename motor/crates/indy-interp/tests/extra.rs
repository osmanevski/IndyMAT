//! Tests for the builtins in `builtins/extra.rs`.

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

fn close(a: &[f64], b: &[f64], tol: f64) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(x, y)| (x - y).abs() <= tol * (1.0 + y.abs()))
}

fn check(src: &str, var: &str, want: &[f64], tol: f64) {
    let it = run(src);
    let got = nums(&it, var);
    assert!(
        close(&got, want, tol),
        "{}: {} = {:?}, want {:?}",
        src,
        var,
        got,
        want
    );
}

fn fails(src: &str) -> String {
    let mut it = Interp::new();
    match it.run_text(src) {
        Ok(_) => panic!("expected an error: {}", src),
        Err(e) => e.identifier.clone(),
    }
}

#[test]
fn sparse_and_structure() {
    check("a = issparse(eye(2)); b = full([1 2]);", "a", &[0.0], 0.0);
    check(
        "s = sparse([1 2 2], [1 2 2], [3 4 5]);",
        "s",
        &[3.0, 0.0, 0.0, 9.0],
        0.0,
    );
    check(
        "t = [isdiag(eye(3)) istril([1 0;2 3]) istriu([1 0;2 3]) issymmetric([1 2;2 1]) ishermitian([1 1i;-1i 2]) issymmetric([1 2;3 4])];",
        "t",
        &[1.0, 1.0, 0.0, 1.0, 1.0, 0.0],
        0.0,
    );
    check("t = issymmetric([1 2;2+1e-12 1], 1e-10);", "t", &[1.0], 0.0);
}

#[test]
fn typecast_and_swapbytes() {
    check(
        "x = typecast(single(1), 'uint8');",
        "x",
        &[0.0, 0.0, 128.0, 63.0],
        0.0,
    );
    check(
        "x = typecast(uint8([0 0 128 63]), 'single');",
        "x",
        &[1.0],
        0.0,
    );
    check("x = typecast(int16(-1), 'uint16');", "x", &[65535.0], 0.0);
    check("x = swapbytes(uint16(1));", "x", &[256.0], 0.0);
    check(
        "x = size(typecast(uint8([1;2;3;4]), 'uint16'));",
        "x",
        &[2.0, 1.0],
        0.0,
    );
}

#[test]
fn binary_file_io() {
    let dir = std::env::temp_dir().join(format!("indy-extra-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("b.bin").display().to_string();
    let src = format!(
        "fid = fopen('{f}', 'w'); n = fwrite(fid, [1 2 300], 'uint16'); fwrite(fid, 'hi'); fclose(fid);\n\
         fid = fopen('{f}'); [a, c] = fread(fid, 3, 'uint16'); p = ftell(fid); s = fread(fid, Inf, '*char')'; \n\
         frewind(fid); b = fread(fid, [2 Inf], 'uint8'); st = fseek(fid, -2, 'eof'); t = fread(fid, 1, 'uint8=>char'); fclose(fid);"
    );
    let it = run(&src);
    assert_eq!(nums(&it, "n"), vec![3.0]);
    assert_eq!(nums(&it, "a"), vec![1.0, 2.0, 300.0]);
    assert_eq!(nums(&it, "c"), vec![3.0]);
    assert_eq!(nums(&it, "p"), vec![6.0]);
    assert_eq!(it.get_var("s").unwrap().to_str().unwrap(), "hi");
    assert_eq!(it.get_var("b").unwrap().dims(), vec![2, 4]);
    assert_eq!(nums(&it, "st"), vec![0.0]);
    assert_eq!(it.get_var("t").unwrap().to_str().unwrap(), "h");
    let src = format!(
        "[fid, name, msg] = mkstemp('{}/tmpXXXXXX'); fwrite(fid, [1 2 3]); fseek(fid, 0, 'bof'); x = fread(fid)'; fclose(fid); ok = isfile(name);",
        dir.display()
    );
    let it = run(&src);
    assert_eq!(nums(&it, "x"), vec![1.0, 2.0, 3.0]);
    assert_eq!(nums(&it, "ok"), vec![1.0]);
    let src = format!(
        "p = file_in_path('/nonexistent:{}', 'b.bin'); q = file_in_path('{}', 'zz.none');",
        dir.display(),
        dir.display()
    );
    let it = run(&src);
    assert_eq!(it.get_var("p").unwrap().to_str().unwrap(), f);
    assert!(it.get_var("q").unwrap().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn misc_helpers() {
    check(
        "x = nextpow2([0 1 5 0.5 1024 1025]);",
        "x",
        &[0.0, 0.0, 3.0, -1.0, 10.0, 11.0],
        0.0,
    );
    check(
        "x = lookup([1 2 3], [NaN 0 1 2.5 9]);",
        "x",
        &[3.0, 0.0, 1.0, 2.0, 3.0],
        0.0,
    );
    check(
        "x = lookup([3 2 1], [0 1 2.5 9]);",
        "x",
        &[3.0, 3.0, 1.0, 0.0],
        0.0,
    );
    check("x = lookup([1 2 3], [1.5 2], 'm');", "x", &[0.0, 2.0], 0.0);
    check(
        "x = unicode2native('aé', 'UTF-8');",
        "x",
        &[97.0, 195.0, 169.0],
        0.0,
    );
    check(
        "x = double(native2unicode(uint8([97 195 169]), 'UTF-8'));",
        "x",
        &[97.0, 233.0],
        0.0,
    );
    check(
        "x = double(do_string_escapes('a\\n\\x41\\101\\\\'));",
        "x",
        &[97.0, 10.0, 65.0, 65.0, 92.0],
        0.0,
    );
    let it = run("s = undo_string_escapes(sprintf('a\\nb\"\\\\'));");
    assert_eq!(it.get_var("s").unwrap().to_str().unwrap(), "a\\nb\\\"\\\\");
    let it = run("s = dbstack; n = numel(s); f = fieldnames(s);");
    assert_eq!(nums(&it, "n"), vec![0.0]);
    check(
        "c = clock; n = numel(c); y = c(1) > 2000;",
        "n",
        &[6.0],
        0.0,
    );
    check(
        "d = datenum(2024, 1, 1); e = datenum([2000 1 1 12 0 0]); f = datenum(2024, 3, 1);",
        "d",
        &[739252.0],
        0.0,
    );
    check("e = datenum([2000 1 1 12 0 0]);", "e", &[730486.5], 0.0);
    check(
        "f = datenum(2024, 3, 1) - datenum(2024, 2, 28);",
        "f",
        &[2.0],
        0.0,
    );
    check(
        "t = now > 7e5; u = cputime >= 0; pause(0); pause(0.01); d = numel(date);",
        "d",
        &[11.0],
        0.0,
    );
}

#[test]
fn factor_rank() {
    check(
        "f = factor(360);",
        "f",
        &[2.0, 2.0, 2.0, 3.0, 3.0, 5.0],
        0.0,
    );
    check("[p, k] = factor(360);", "k", &[3.0, 2.0, 1.0], 0.0);
    check("f = factor(97);", "f", &[97.0], 0.0);
    check(
        "r = [rank([16 2 3 13;5 11 10 8;9 7 6 12;4 14 15 1]) rank(eye(3)) rank([1 2;2 4]) rank(zeros(2)) rank([1 1i; 1i -1])];",
        "r",
        &[3.0, 3.0, 1.0, 0.0, 1.0],
        0.0,
    );
}

#[test]
fn eig_family() {
    check("e = eig([2 1;1 2]);", "e", &[1.0, 3.0], 1e-12);
    check(
        "e = eig([1 2;3 4]);",
        "e",
        &[-0.372_281_323_269_014_3, 5.372_281_323_269_014],
        1e-12,
    );
    check(
        "e = eig([4 1 0;1 4 1;0 1 4]);",
        "e",
        &[4.0 - 2f64.sqrt(), 4.0, 4.0 + 2f64.sqrt()],
        1e-12,
    );
    check(
        "M = [16 2 3 13;5 11 10 8;9 7 6 12;4 14 15 1]; e = sort(eig(M));",
        "e",
        &[-8.944_271_909_999_16, -2.2e-15, 8.944_271_909_999_16, 34.0],
        1e-9,
    );
    check(
        "e = eig([0 1;-1 0]); r = real(e); i = imag(e);",
        "i",
        &[1.0, -1.0],
        1e-12,
    );
    check(
        "A = [1 2 3;4 5 6;7 8 10]; [V, D] = eig(A); r = norm(A*V - V*D, 1);",
        "r",
        &[0.0],
        1e-9,
    );
    check(
        "A = [2 1;1 2]; [V, D] = eig(A); r = norm(A*V - V*D, 1);",
        "r",
        &[0.0],
        1e-12,
    );
    check(
        "A = [0 1;-2 -3]; [V, D] = eig(A); r = abs(A*V - V*D); r = max(r(:));",
        "r",
        &[0.0],
        1e-9,
    );
    check("A = [1 1i;-1i 1]; e = eig(A);", "e", &[0.0, 2.0], 1e-12);
    check(
        "A = [2 0;0 3]; B = [2 0;0 1]; e = eig(A, B);",
        "e",
        &[1.0, 3.0],
        1e-12,
    );
    check("r = roots([1 -3 2]);", "r", &[2.0, 1.0], 1e-12);
    check("r = roots([1 0 0]);", "r", &[0.0, 0.0], 0.0);
    check("p = poly([1 2]);", "p", &[1.0, -3.0, 2.0], 1e-12);
    check("p = poly([1 2;3 4]);", "p", &[1.0, -5.0, -2.0], 1e-12);
    check("p = poly([1i -1i]); r = isreal(p);", "r", &[1.0], 0.0);
    check(
        "E = expm([0 1;-1 0]);",
        "E",
        &[1f64.cos(), -(1f64.sin()), 1f64.sin(), 1f64.cos()],
        1e-13,
    );
    check("E = expm(zeros(2));", "E", &[1.0, 0.0, 0.0, 1.0], 0.0);
    check(
        "E = expm([1 2;0 1]*10); r = E(1,2) / (20*exp(10));",
        "r",
        &[1.0],
        1e-12,
    );
    assert_eq!(fails("eig([1 2 3])"), "MATLAB:eig:inputMustBeSquareMatrix");
    assert_eq!(fails("eig([NaN 1;1 1])"), "MATLAB:eig:matrixWithNaNInf");
}

#[test]
fn reductions() {
    check("v = vecnorm([3 4;0 0]);", "v", &[3.0, 4.0], 1e-15);
    check("v = vecnorm([3 4;0 0], 2, 2);", "v", &[5.0, 0.0], 1e-15);
    check("v = vecnorm([1 -2 3], Inf);", "v", &[3.0], 0.0);
    check("m = mode([1 2 2 3 3]);", "m", &[2.0], 0.0);
    check("[m, f] = mode([1 2 2; 3 3 1]);", "f", &[1.0, 1.0, 1.0], 0.0);
    check("[m, f] = mode([4 4 NaN NaN NaN]);", "f", &[2.0], 0.0);
    check("[lo, hi] = bounds([3 NaN -1 7]);", "lo", &[-1.0], 0.0);
    check("[lo, hi] = bounds([3 NaN -1 7]);", "hi", &[7.0], 0.0);
    check("r = rescale([1 2 3]);", "r", &[0.0, 0.5, 1.0], 1e-15);
    check(
        "r = rescale([1 2 3], -1, 1);",
        "r",
        &[-1.0, 0.0, 1.0],
        1e-15,
    );
    check(
        "n = histcounts([1 2 2 3 5], [0 2 4 6]);",
        "n",
        &[1.0, 3.0, 1.0],
        0.0,
    );
    check(
        "[n, e] = histcounts([1 2 3 4 5 6 7 8 9 10], 3);",
        "n",
        &[3.0, 4.0, 3.0],
        0.0,
    );
    check(
        "[n, e] = histcounts([1 2 3 4 5 6 7 8 9 10], 3);",
        "e",
        &[0.0, 4.0, 8.0, 12.0],
        0.0,
    );
    check(
        "[n, e] = histcounts([1 2 2 3 4], 'BinWidth', 1); s = sum(n);",
        "s",
        &[5.0],
        0.0,
    );
    check(
        "n = histcounts([1 2 3], [0 2 4], 'Normalization', 'probability');",
        "n",
        &[1.0 / 3.0, 2.0 / 3.0],
        1e-15,
    );
    check(
        "[n, e] = histcounts(randn(1, 1000)); s = sum(n);",
        "s",
        &[1000.0],
        0.0,
    );
}

#[test]
fn signals_and_polynomials() {
    check(
        "x = filter(1, [1 -0.5], [1 1 1]);",
        "x",
        &[1.0, 1.5, 1.75],
        1e-15,
    );
    check("[y, zf] = filter([1 1], 1, [1 2 3]);", "zf", &[3.0], 0.0);
    check(
        "y = filter([1 1], 1, [1 2;3 4]);",
        "y",
        &[1.0, 4.0, 2.0, 6.0],
        0.0,
    );
    check(
        "y = filter(1, [2 -1], [2 2], 0.5);",
        "y",
        &[1.5, 1.75],
        1e-15,
    );
    check("w = conv([1 2 3], [1 1]);", "w", &[1.0, 3.0, 5.0, 3.0], 0.0);
    check(
        "w = conv([1 2 3], [1 1], 'same');",
        "w",
        &[3.0, 5.0, 3.0],
        0.0,
    );
    check("w = conv([1 2 3], [1 1], 'valid');", "w", &[3.0, 5.0], 0.0);
    check("s = size(conv([1;2], [1 2 3]));", "s", &[1.0, 4.0], 0.0);
    check(
        "[q, r] = deconv([1 5 6 1], [1 2]);",
        "q",
        &[1.0, 3.0, 0.0],
        0.0,
    );
    check(
        "[q, r] = deconv([1 5 6 1], [1 2]);",
        "r",
        &[0.0, 0.0, 0.0, 1.0],
        0.0,
    );
    check(
        "y = polyval([1 2 3], [0 1 2]);",
        "y",
        &[3.0, 6.0, 11.0],
        0.0,
    );
    check("g = gradient([1 4 9 16]);", "g", &[3.0, 4.0, 6.0, 7.0], 0.0);
    check(
        "g = gradient([1 4 9 16], 2);",
        "g",
        &[1.5, 2.0, 3.0, 3.5],
        0.0,
    );
    check(
        "[gx, gy] = gradient([1 2 4; 2 4 8]);",
        "gx",
        &[1.0, 2.0, 1.5, 3.0, 2.0, 4.0],
        0.0,
    );
    check(
        "[gx, gy] = gradient([1 2 4; 2 4 8]);",
        "gy",
        &[1.0, 1.0, 2.0, 2.0, 4.0, 4.0],
        0.0,
    );
    check("q = trapz([1 2 3]);", "q", &[4.0], 0.0);
    check("q = trapz([0 1 3], [1 2 3]);", "q", &[6.5], 0.0);
    check("q = trapz([1 2;3 4]);", "q", &[2.0, 3.0], 0.0);
    check("q = trapz([1 2;3 4], 2);", "q", &[1.5, 3.5], 0.0);
    check("q = cumtrapz([1 2 3]);", "q", &[0.0, 1.5, 4.0], 0.0);
    check(
        "m = movmean([1 2 3 4 5], 3);",
        "m",
        &[1.5, 2.0, 3.0, 4.0, 4.5],
        1e-15,
    );
    check(
        "m = movmean([1 2 3 4 5], 2);",
        "m",
        &[1.0, 1.5, 2.5, 3.5, 4.5],
        1e-15,
    );
    check(
        "m = movsum([1 2 3 4 5], 3);",
        "m",
        &[3.0, 6.0, 9.0, 12.0, 9.0],
        0.0,
    );
    check(
        "m = movsum([1 2 3 4 5], 3, 'Endpoints', 'discard');",
        "m",
        &[6.0, 9.0, 12.0],
        0.0,
    );
    check(
        "m = movmean([1 NaN 3], 3, 'omitnan');",
        "m",
        &[1.0, 2.0, 3.0],
        0.0,
    );
    check("r = rmmissing([1 NaN 3]);", "r", &[1.0, 3.0], 0.0);
    check(
        "r = rmmissing([1 2; NaN 3; 4 5]);",
        "r",
        &[1.0, 4.0, 2.0, 5.0],
        0.0,
    );
    check("[r, tf] = rmmissing([1; NaN]);", "tf", &[0.0, 1.0], 0.0);
}

#[test]
fn solvers_and_quadrature() {
    check(
        "x = fzero(@(x) x.^2 - 2, [0 2]);",
        "x",
        &[2f64.sqrt()],
        1e-14,
    );
    check(
        "x = fzero(@cos, 1);",
        "x",
        &[std::f64::consts::FRAC_PI_2],
        1e-14,
    );
    check(
        "[x, fv, flag] = fzero(@(x) x - 3, 10);",
        "flag",
        &[1.0],
        0.0,
    );
    check("x = fzero('sin', 3);", "x", &[std::f64::consts::PI], 1e-14);
    assert_eq!(
        fails("fzero(@(x) x.^2 + 1, [0 1])"),
        "MATLAB:fzero:ValuesAtEndPtsSameSign"
    );
    check(
        "o = optimset('TolX', 1e-3); t = optimget(o, 'tolx'); d = optimget(o, 'MaxIter', 7);",
        "t",
        &[1e-3],
        0.0,
    );
    check(
        "o = optimset('TolX', 1e-3); d = optimget(o, 'MaxIter', 7);",
        "d",
        &[7.0],
        0.0,
    );
    check(
        "o = optimset(optimset('TolX', 1e-3), 'Display', 'off'); t = optimget(o, 'TolX');",
        "t",
        &[1e-3],
        0.0,
    );
    check("x = fminbnd(@(x) (x - 1).^2, -3, 3);", "x", &[1.0], 1e-4);
    check(
        "[x, fv] = fminbnd(@cos, 3, 4);",
        "x",
        &[std::f64::consts::PI],
        1e-4,
    );
    check("q = integral(@(x) x.^2, 0, 1);", "q", &[1.0 / 3.0], 1e-12);
    check(
        "q = integral(@(x) exp(-x.^2), -Inf, Inf);",
        "q",
        &[std::f64::consts::PI.sqrt()],
        1e-9,
    );
    check("q = integral(@(x) 1 ./ x.^2, 1, Inf);", "q", &[1.0], 1e-9);
    check("q = integral(@(x) 5, 0, 2);", "q", &[10.0], 1e-12);
    check(
        "q = integral(@(x) [x, 2*x], 0, 1, 'ArrayValued', true);",
        "q",
        &[0.5, 1.0],
        1e-12,
    );
    check("q = integral(@(x) sin(x), pi, 0);", "q", &[-2.0], 1e-12);
    check("q = quad(@(x) x.^3, 0, 2);", "q", &[4.0], 1e-9);
    check("[q, e] = quadgk(@(x) log(x), 0, 1);", "q", &[-1.0], 1e-8);
    check(
        "q = integral(@(x) abs(x - 0.3), 0, 1, 'Waypoints', 0.3);",
        "q",
        &[0.29],
        1e-12,
    );
}

#[test]
fn bad_inputs_do_not_panic() {
    let cases = [
        "typecast({1}, 'uint8')",
        "typecast(uint8([1 2 3]), 'uint16')",
        "fread(99)",
        "fwrite(99, 1)",
        "fseek(99, 0, 'bof')",
        "factor(-1)",
        "factor(1.5)",
        "rank({1})",
        "expm([1 2 3])",
        "filter([], 1, 1)",
        "filter(1, 0, 1)",
        "conv(ones(2), 1)",
        "deconv(1, 0)",
        "roots([1 NaN])",
        "movmean(1:3, 0)",
        "histcounts(1:3, 'Bogus', 1)",
        "integral(@(x) [x x], 0, 1)",
        "datenum('not a date')",
        "lookup({1}, 'a')",
        "mkstemp(1)",
    ];
    for c in cases {
        let mut it = Interp::new();
        assert!(it.run_text(c).is_err(), "{} should fail", c);
    }
    let ok = [
        "x = eig([]);",
        "x = expm([]);",
        "x = histcounts([]);",
        "x = mode([]);",
        "x = trapz([]);",
        "x = rmmissing([]);",
        "x = conv([], [1 2]);",
        "x = roots([]);",
        "x = poly([]);",
        "x = vecnorm([]);",
        "x = gradient([]);",
        "x = filter(1, 1, []);",
        "x = movmean([], 3);",
        "x = cumtrapz([]);",
        "x = mkstemp('abc');",
        "x = rank([]);",
        "x = nextpow2([]);",
        "x = lookup([], 1);",
    ];
    for c in ok {
        let mut it = Interp::new();
        if let Err(e) = it.run_text(c) {
            panic!("{} failed: {}", c, e.message);
        }
    }
}
