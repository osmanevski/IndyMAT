//! Sparse matrices: storage rules, operators, indexing, constructors. The
//! expected storage and values were compared with Octave 11 where Octave
//! and MATLAB's documented rules agree.

use indy_interp::Interp;
use indy_value::Value;

fn run(code: &str) -> Interp {
    let mut it = Interp::new();
    if let Err(e) = it.run_text(code) {
        panic!("{}: {}", code, e.message);
    }
    it
}

fn eval(code: &str) -> Value {
    run(&format!("r = {};", code))
        .get_var("r")
        .expect("r")
        .clone()
}

/// Storage and full values of an expression.
fn check(code: &str, sparse: bool, dims: &[usize], values: &[f64]) {
    let v = eval(code);
    assert_eq!(v.is_sparse(), sparse, "storage of {}", code);
    assert_eq!(v.dims(), dims, "size of {}", code);
    let got = v.to_f64_vec().expect("numeric");
    assert_eq!(got, values, "values of {}", code);
}

fn fails(code: &str) {
    assert!(Interp::new().run_text(code).is_err(), "{}", code);
}

const S: &str = "sparse([1 2],[1 2],[5 -7],2,2)";

#[test]
fn constructors() {
    check("sparse([1 0;0 2])", true, &[2, 2], &[1.0, 0.0, 0.0, 2.0]);
    check("sparse(2,3)", true, &[2, 3], &[0.0; 6]);
    check(
        "sparse([1 1 2],[1 1 2],[2 3 4])",
        true,
        &[2, 2],
        &[5.0, 0.0, 0.0, 4.0],
    );
    check("sparse(1:2,1:2,1)", true, &[2, 2], &[1.0, 0.0, 0.0, 1.0]);
    check(
        "sparse([1 2],[1 2],[1 2],3,3,5)",
        true,
        &[3, 3],
        &[1.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0],
    );
    check("nnz(sparse([1 1],[1 1],[1 -1]))", false, &[1, 1], &[0.0]);
    check(
        "class(sparse([1 1],[1 1],[true true]))=='logical'",
        false,
        &[1, 7],
        &[1.0; 7],
    );
    check("speye(2,3)", true, &[2, 3], &[1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    check("spalloc(2,2,5)", true, &[2, 2], &[0.0; 4]);
    check("spones(sparse([2 0 -3]))", true, &[1, 3], &[1.0, 0.0, 1.0]);
}

#[test]
fn constructor_errors() {
    fails("sparse(int8(1));");
    fails("sparse('a');");
    fails("sparse(ones(2,2,2));");
    fails("sparse(0,1,1);");
    fails("sparse(1.5,1,1);");
    fails("sparse([1 2],[1 2 3],1);");
    fails("sparse(3,1,1,2,2);");
    fails("sparse(1:3,1:3,1,3,3,2);");
    fails("sparse(1,2,3,4);");
    fails("sparse(1,3e9);");
}

#[test]
fn result_storage_follows_matlab() {
    for (code, sparse) in [
        ("S+S", true),
        ("S-S", true),
        ("S+F", false),
        ("F-S", false),
        ("S+1", false),
        ("S*2", true),
        ("2*S", true),
        ("S/2", true),
        ("S.*F", true),
        ("F.*S", true),
        ("S./F", true),
        ("F./S", false),
        ("1./S", false),
        ("S.^2", true),
        ("S.^0", false),
        ("2.^S", false),
        ("S*S", true),
        ("S*F", false),
        ("F*S", false),
        ("S^2", true),
        ("S\\[1;2]", false),
        ("S'", true),
        ("-S", true),
        ("~S", true),
        ("S>0", true),
        ("S==0", true),
        ("S>F", true),
        ("S&F", true),
        ("S|F", false),
        ("S|S", true),
        ("abs(S)", true),
        ("sum(S)", true),
        ("max(S)", true),
        ("any(S)", true),
        ("cos(S)", true),
        ("exp(S)", true),
        ("mod(S,2)", true),
        ("diag(S)", true),
        ("tril(S)", true),
        ("kron(S,S)", true),
        ("inv(S)", true),
        ("repmat(S,1,2)", true),
        ("S(1,1)", true),
        ("S(:,1)", true),
        ("S(:)", true),
        ("S([1 4])", true),
        ("[S S]", true),
        ("[S;F]", true),
        ("[F,S]", true),
        ("reshape(S,1,4)", true),
        ("permute(S,[2 1])", true),
        ("full(S)", false),
        ("find(S)", false),
        ("nonzeros(S)", false),
        ("nnz(S)", false),
        ("norm(S,1)", false),
        ("det(S)", false),
        ("single(S)", false),
    ] {
        let it = run(&format!("S = {}; F = [1 2;3 4]; r = {};", S, code));
        assert_eq!(it.get_var("r").unwrap().is_sparse(), sparse, "{}", code);
    }
}

#[test]
fn arithmetic_values() {
    let d = S;
    check(&format!("{d}+{d}"), true, &[2, 2], &[10.0, 0.0, 0.0, -14.0]);
    check(&format!("{d}-{d}"), true, &[2, 2], &[0.0; 4]);
    check(&format!("{d}*{d}"), true, &[2, 2], &[25.0, 0.0, 0.0, 49.0]);
    check(&format!("{d}*[1;1]"), false, &[2, 1], &[5.0, -7.0]);
    check(&format!("[1 1]*{d}"), false, &[1, 2], &[5.0, -7.0]);
    check(&format!("{d}\\[5;-7]"), false, &[2, 1], &[1.0, 1.0]);
    check(&format!("sum({d},2)"), true, &[2, 1], &[5.0, -7.0]);
    // Zeros take part when the other operand is NaN or the divisor is zero.
    let v = eval("sparse([1 0])*NaN").to_f64_vec().unwrap();
    assert!(v.iter().all(|x| x.is_nan()));
    let v = eval("sparse([1 0])/0").to_f64_vec().unwrap();
    assert!(v[0].is_infinite() && v[1].is_nan());
}

#[test]
fn complex_values() {
    let it = run("C = sparse(1+2i); a = isreal(C); b = imag(C'); c = real(C*C); d = imag(C*C);");
    assert_eq!(it.get_var("a").unwrap().scalar_f64(), Some(0.0));
    assert_eq!(it.get_var("b").unwrap().scalar_f64(), Some(-2.0));
    assert_eq!(it.get_var("c").unwrap().scalar_f64(), Some(-3.0));
    assert_eq!(it.get_var("d").unwrap().scalar_f64(), Some(4.0));
}

#[test]
fn logical_values() {
    let it = run(
        "L = sparse([1 0])>0; a = islogical(L); b = isnumeric(L); c = class(L+L); d = isa(L,'logical'); e = isa(sparse(1),'numeric');",
    );
    assert_eq!(it.get_var("a").unwrap().scalar_f64(), Some(1.0));
    assert_eq!(it.get_var("b").unwrap().scalar_f64(), Some(0.0));
    assert_eq!(it.get_var("c").unwrap().to_str().unwrap(), "double");
    assert_eq!(it.get_var("d").unwrap().scalar_f64(), Some(1.0));
    assert_eq!(it.get_var("e").unwrap().scalar_f64(), Some(1.0));
}

#[test]
fn indexing() {
    check("speye(3)(2,2)", true, &[1, 1], &[1.0]);
    check("speye(3)(:,2)", true, &[3, 1], &[0.0, 1.0, 0.0]);
    check("speye(3)(2,:)", true, &[1, 3], &[0.0, 1.0, 0.0]);
    check("speye(2)(:)", true, &[4, 1], &[1.0, 0.0, 0.0, 1.0]);
    check("speye(2)([1 2;3 4])", true, &[2, 2], &[1.0, 0.0, 0.0, 1.0]);
    check("speye(2)(speye(2)>0)", true, &[2, 1], &[1.0, 1.0]);
    fails("speye(2)(3,3);");
    fails("speye(2)(9);");
}

#[test]
fn assignment() {
    let get = |code: &str| {
        let it = run(&format!("S = speye(2); {} r = S;", code));
        let r = it.get_var("r").unwrap().clone();
        assert!(r.is_sparse(), "{}", code);
        (r.dims(), r.to_f64_vec().unwrap())
    };
    assert_eq!(get("S(1,2) = 7;").1, vec![1.0, 0.0, 7.0, 1.0]);
    assert_eq!(get("S(1,1) = 0;").1, vec![0.0, 0.0, 0.0, 1.0]);
    assert_eq!(get("S(:,2) = [3;4];").1, vec![1.0, 0.0, 3.0, 4.0]);
    assert_eq!(get("S(4) = 9;").1, vec![1.0, 0.0, 0.0, 9.0]);
    assert_eq!(get("S(S>0) = 2;").1, vec![2.0, 0.0, 0.0, 2.0]);
    assert_eq!(get("S(3,3) = 5;").0, vec![3, 3]);
    assert_eq!(get("S(:,1) = [];").0, vec![2, 1]);
    assert_eq!(get("S([1 1],1) = [4;5];").1, vec![5.0, 0.0, 0.0, 1.0]);
    // A full target stays full.
    let it = run("A = eye(2); A(1,2) = sparse(3);");
    assert!(!it.get_var("A").unwrap().is_sparse());
    // Logical stays logical; a failed assignment keeps the value.
    let it = run("L = speye(2)>0; L(1,2) = 5; a = class(L); b = full(L(1,2));");
    assert_eq!(it.get_var("a").unwrap().to_str().unwrap(), "logical");
    assert_eq!(it.get_var("b").unwrap().scalar_f64(), Some(1.0));
    let it = run("S = speye(2); try, S(1:2,1) = [1 2 3]; catch, end; n = nnz(S);");
    assert_eq!(it.get_var("n").unwrap().scalar_f64(), Some(2.0));
}

#[test]
fn find_and_nonzeros() {
    let it = run(
        "[i,j,v] = find(sparse([0 2;3 0])); k = find(sparse([0 1 1])); n = nonzeros(sparse([0 2;3 0])); f = find(sparse([0 1 1;1 0 0]),2);",
    );
    assert_eq!(
        it.get_var("i").unwrap().to_f64_vec().unwrap(),
        vec![2.0, 1.0]
    );
    assert_eq!(
        it.get_var("j").unwrap().to_f64_vec().unwrap(),
        vec![1.0, 2.0]
    );
    assert_eq!(
        it.get_var("v").unwrap().to_f64_vec().unwrap(),
        vec![3.0, 2.0]
    );
    assert_eq!(it.get_var("k").unwrap().dims(), vec![1, 2]);
    assert_eq!(
        it.get_var("n").unwrap().to_f64_vec().unwrap(),
        vec![3.0, 2.0]
    );
    assert_eq!(
        it.get_var("f").unwrap().to_f64_vec().unwrap(),
        vec![2.0, 3.0]
    );
}

#[test]
fn spdiags_forms() {
    let it = run("[B,d] = spdiags(sparse([1 2 0;0 3 4;5 0 6]));");
    assert_eq!(
        it.get_var("d").unwrap().to_f64_vec().unwrap(),
        vec![-2.0, 0.0, 1.0]
    );
    assert_eq!(
        it.get_var("B").unwrap().to_f64_vec().unwrap(),
        vec![5.0, 0.0, 0.0, 1.0, 3.0, 6.0, 0.0, 2.0, 4.0]
    );
    check("spdiags([1;2],1,2,2)", true, &[2, 2], &[0.0, 0.0, 2.0, 0.0]);
    check(
        "spdiags([1;2],0,sparse([0 9;0 0]))",
        true,
        &[2, 2],
        &[1.0, 0.0, 9.0, 2.0],
    );
    fails("spdiags(ones(1,3),[0 1],3,3);");
    fails("spdiags(ones(2,1),0,3,3);");
}

#[test]
fn random_is_seeded() {
    let it = run(
        "rng(7); a = sprand(20,20,0.2); rng(7); b = sprand(20,20,0.2); c = sprandn(20,20,0.2); same = isequal(a,b); sp = issparse(c); low = all(nonzeros(a) > 0 & nonzeros(a) < 1); n = nnz(a);",
    );
    assert_eq!(it.get_var("same").unwrap().scalar_f64(), Some(1.0));
    assert_eq!(it.get_var("sp").unwrap().scalar_f64(), Some(1.0));
    assert_eq!(it.get_var("low").unwrap().scalar_f64(), Some(1.0));
    let n = it.get_var("n").unwrap().scalar_f64().unwrap();
    assert!(n > 40.0 && n <= 80.0, "{}", n);
    fails("sprand(2,2,2);");
    fails("sprand(2,2,0.5,0.1);");
}

#[test]
fn huge_matrices_stay_on_the_storage() {
    let it = run(
        "n = 100000; S = speye(n); T = S + S*S; a = nnz(T); b = full(sum(sum(T))); S(3,4) = 1; c = nnz(S); d = nnz(S(:)); e = nnz(diag(S)); f = nnz([S S]);",
    );
    assert_eq!(it.get_var("a").unwrap().scalar_f64(), Some(100000.0));
    assert_eq!(it.get_var("b").unwrap().scalar_f64(), Some(200000.0));
    assert_eq!(it.get_var("c").unwrap().scalar_f64(), Some(100001.0));
    assert_eq!(it.get_var("d").unwrap().scalar_f64(), Some(100001.0));
    assert_eq!(it.get_var("e").unwrap().scalar_f64(), Some(100000.0));
    assert_eq!(it.get_var("f").unwrap().scalar_f64(), Some(200002.0));
    // What needs the full form says so instead of exhausting memory.
    fails("full(speye(100000));");
    fails("speye(100000)\\ones(100000,1);");
    fails("S = speye(2); S(1,3e9) = 1;");
}

#[test]
fn containers_and_handles_keep_storage() {
    let it = run(
        "c = {speye(2)}; s.a = speye(2); f = @(x) 2*x; a = issparse(c{1}); b = issparse(s.a); d = issparse(f(speye(2))); e = cellfun(@issparse,{speye(2),eye(2)}); g = isequal(speye(2),eye(2)); h = isequaln(sparse(NaN),sparse(NaN)); k = isequal(sparse(NaN),sparse(NaN));",
    );
    for n in ["a", "b", "d", "g", "h"] {
        assert_eq!(it.get_var(n).unwrap().scalar_f64(), Some(1.0), "{}", n);
    }
    assert_eq!(
        it.get_var("e").unwrap().to_f64_vec().unwrap(),
        vec![1.0, 0.0]
    );
    assert_eq!(it.get_var("k").unwrap().scalar_f64(), Some(0.0));
}

#[test]
fn display_layout() {
    let text = indy_value::format::display_text("T", &eval("sparse([1 2],[1 2],[5 7],3,3)"));
    assert_eq!(text, "T =\n\n   (1,1)        5\n   (2,2)        7\n\n");
    let text = indy_value::format::disp_text(&eval("sparse(1,1,0.5)"));
    assert_eq!(text, "   (1,1)       0.5000\n");
}

#[test]
fn truth_values() {
    let it = run(
        "if speye(2), a = 1; else, a = 0; end; if sparse(ones(2)), b = 1; else, b = 0; end; if sparse(2,2), c = 1; else, c = 0; end;",
    );
    assert_eq!(it.get_var("a").unwrap().scalar_f64(), Some(0.0));
    assert_eq!(it.get_var("b").unwrap().scalar_f64(), Some(1.0));
    assert_eq!(it.get_var("c").unwrap().scalar_f64(), Some(0.0));
}

#[test]
fn scalars_in_control_flow_and_as_arguments() {
    let it = run(
        "S = sparse([1 0;0 3]); a = 0; if S(1,1) > 0 && S(2,2) == 3, a = 1; end; b = numel(1:S(2,2)); v = [10 20 30]; c = v(S(2,2)); d = zeros(1,S(1,1)); e = \"n=\" + sparse(4);",
    );
    assert_eq!(it.get_var("a").unwrap().scalar_f64(), Some(1.0));
    assert_eq!(it.get_var("b").unwrap().scalar_f64(), Some(3.0));
    assert_eq!(it.get_var("c").unwrap().scalar_f64(), Some(30.0));
    assert_eq!(it.get_var("d").unwrap().dims(), vec![1, 1]);
    assert_eq!(it.get_var("e").unwrap().to_str().unwrap(), "n=4");
}

// ---------------------------------------------------------------------------
// Paths found by the audit of everything that receives a sparse value
// ---------------------------------------------------------------------------

/// Each statement list must leave `ok` true.
fn holds(code: &str) {
    let it = run(&format!(
        "S = {S}; L = sparse(logical([1 0;0 1])); C = sparse([1 2],[1 2],[1+2i 3],2,2); {code}"
    ));
    assert_eq!(it.get_var("ok"), Some(&Value::bool(true)), "{code}");
}

#[test]
fn builtin_function_keeps_the_sparse_rules() {
    holds("ok = builtin('issparse', S);");
    holds("ok = issparse(builtin('sum', S)) && isequal(builtin('sum', S), [5 -7]);");
    holds("ok = builtin('nnz', S) == 2;");
    holds("ok = isequal(builtin('size', S), [2 2]);");
}

#[test]
fn a_cell_keeps_sparse_values_in_concatenation() {
    holds("c = [S, {1}]; ok = issparse(c{1}) && isequal(c{1}, S) && isequal(size(c), [1 2]);");
    holds("c = [{1}; S]; ok = issparse(c{2}) && isequal(size(c), [2 1]);");
    holds("c = cat(2, {1}, L); ok = issparse(c{2}) && islogical(c{2});");
}

#[test]
fn n_dimensional_concatenation_is_refused() {
    fails("cat(3, sparse(1), sparse(1))");
    fails("cat(3, sparse(1), 1)");
}

#[test]
fn text_inside_cells_and_tables() {
    holds("x = string({sparse(5)}); ok = x == \"5\";");
    holds(
        "t = table([1;2]); t.b = S(:,1); x = evalc('disp(t)'); ok = ~isempty(strfind(x, '5')) && isempty(strfind(x, '1\u{d7}1'));",
    );
    let d = std::env::temp_dir().join(format!("indy_sparse_table_{}.csv", std::process::id()));
    let p = d.display();
    holds(&format!(
        "t = table([1;2]); t.b = S(:,1); writetable(t, '{p}'); x = fileread('{p}'); delete('{p}'); ok = ~isempty(strfind(x, '1,5')) && ~isempty(strfind(x, '2,0'));"
    ));
}

#[test]
fn cellfun_and_arrayfun_see_the_class() {
    holds("ok = isequal(cellfun('isnumeric', {S, L}), [true false]);");
    holds("ok = isequal(cellfun('islogical', {S, L}), [false true]);");
    holds("ok = isequal(cellfun('isreal', {S, C}), [true false]);");
    holds("ok = isequal(cellfun(@isnumeric, {S, L}), [true false]);");
    holds("ok = isequal(cellfun(@(x) x(1,1), {S}), 5);");
    holds("x = cellfun(@(x) L(1,1), {1}); ok = islogical(x) && x;");
    holds("x = arrayfun(@(x) x, L); ok = islogical(x) && isequal(x, full(L));");
    holds("ok = isequal(arrayfun(@(x) issparse(x), S), true(2));");
    holds("x = arrayfun(@(x) x, C); ok = isequal(x, full(C));");
    fails("arrayfun(@(x) x, speye(1000000))");
}

#[test]
fn jsonencode_of_sparse() {
    holds("ok = strcmp(jsonencode(S), '[[5,0],[0,-7]]');");
    holds("ok = strcmp(jsonencode(struct('a', S)), '{\"a\":[[5,0],[0,-7]]}');");
    holds("ok = strcmp(jsonencode({L}), '[[[true,false],[false,true]]]');");
    // The imaginary part must not vanish.
    fails("jsonencode(sparse(1i))");
    fails("jsonencode(struct('a', sparse(1i)))");
    fails("jsonencode(1+2i)");
}

#[test]
fn class_questions() {
    holds("ok = iscomplex(C) && ~iscomplex(S) && ~iscomplex(L) && ~iscomplex(1) && iscomplex(1i);");
    holds("ok = ~iscomplex(speye(1000000));");
}

#[test]
fn result_storage_follows_the_right_argument() {
    holds("x = repmat([1 2], sparse(2), 1); ok = ~issparse(x) && isequal(x, [1 2; 1 2]);");
    holds("ok = issparse(repmat(S, 2, 1));");
    holds("ok = issparse(max(1, S));");
    holds("ok = issparse(squeeze(S)) && isequal(squeeze(S), S);");
    holds("x = xor(S, L); ok = issparse(x) && nnz(x) == 0;");
    holds("x = max(sparse(0, 3)); ok = isequal(size(x), [0 3]);");
    holds(
        "x = max(zeros(0, 3)); y = x + 1; ok = isequal(size(x), [0 3]) && isequal(size(y), [0 3]);",
    );
    holds("[x, k] = min(zeros(0, 3)); ok = isequal(size(k), [0 3]);");
}

#[test]
fn values_pass_through_unchanged() {
    holds("assignin('base', 'q', S); ok = issparse(q);");
    holds("m = containers.Map('k', S); ok = issparse(m('k'));");
    holds("p = inputParser; p.addParameter('a', 1); p.parse('a', S); ok = issparse(p.Results.a);");
}

#[test]
fn callbacks_may_return_sparse() {
    holds("ok = abs(fzero(@(x) sparse(x - 1), [0 3]) - 1) < 1e-6;");
    holds("ok = abs(fminsearch(@(x) sparse((x - 1)^2), 0) - 1) < 1e-3;");
    holds("ok = abs(fminbnd(@(x) sparse((x - 1)^2), 0, 3) - 1) < 1e-3;");
    holds("ok = abs(fsolve(@(x) sparse(x - 1), 0) - 1) < 1e-3;");
    holds("ok = abs(fminunc(@(x) sparse((x - 1)^2), 0) - 1) < 1e-3;");
    holds("ok = abs(integral(@(x) sparse(x), 0, 1) - 0.5) < 1e-6;");
    holds("ok = abs(quadgk(@(x) sparse(x), 0, 1) - 0.5) < 1e-6;");
    holds("ok = abs(integral2(@(x, y) sparse(x + y), 0, 1, 0, 1) - 1) < 1e-6;");
}
