//! MAT files: `save`, `load`, `who`, `whos`. Each test runs MATLAB code in a
//! fresh interpreter whose current folder is a new temporary folder.
//! `matfile_xc/` holds files written by `octave-cli` (11.3) from
//! `matfile_xc/vals.m`; `scripts/matfile_xc.sh` regenerates them and checks
//! the other direction (Octave loading the Rust engine's files).

use indy_interp::Interp;
use std::path::{Path, PathBuf};

fn temp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "indy_matfile_{}_{}_{}",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|t| t.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn run_in(dir: &Path, src: &str) {
    let mut it = Interp::new();
    it.set_cwd(dir.to_path_buf());
    if let Err(e) = it.run_text(src) {
        panic!("{}\n  -> {} ({})", src, e.message, e.identifier);
    }
}

fn run(tag: &str, src: &str) {
    let d = temp_dir(tag);
    run_in(&d, src);
    let _ = std::fs::remove_dir_all(&d);
}

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/matfile_xc")
}

#[test]
fn scoreboard_case_dosya_mat() {
    run(
        "case",
        r#"dosya = [tempname() '.mat'];
        x = magic(3); ad = 'deneme'; s = struct('a', {1, 2});
        save(dosya, 'x', 'ad', 's');
        y = load(dosya); delete(dosya);
        assert(isequal(y.x, magic(3)) && strcmp(y.ad, 'deneme') && isequal(y.s, s));"#,
    );
}

#[test]
fn command_and_function_forms() {
    run(
        "forms",
        r#"a = 1; b = 'iki'; c = {3};
        save hepsi
        assert(exist('hepsi.mat', 'file') == 2);
        save secim a c
        clear all
        load secim
        assert(isequal(a, 1) && isequal(c, {3}) && ~exist('b', 'var'));
        clear all
        load hepsi b
        assert(strcmp(b, 'iki') && ~exist('a', 'var'));
        S = load('hepsi.mat');
        assert(isequal(sort(fieldnames(S)), {'a'; 'b'; 'c'}));
        S = load('hepsi', 'a', 'c');
        assert(isequal(fieldnames(S), {'a'; 'c'}));
        x1 = 1; x2 = 2; y1 = 3;
        save('desen.mat', 'x*');
        S = load('desen.mat'); assert(isequal(fieldnames(S), {'x1'; 'x2'}));
        S = load('desen.mat', '-regexp', '2$'); assert(isequal(fieldnames(S), {'x2'}));
        save('re.mat', '-regexp', '^[xy]1$');
        S = load('re.mat'); assert(isequal(fieldnames(S), {'x1'; 'y1'}));
        save('bos.mat', 'nonexistent*');
        S = load('bos.mat'); assert(isempty(fieldnames(S)));"#,
    );
}

#[test]
fn types_round_trip_v7_v6_v4() {
    run(
        "types",
        r#"d = magic(4); e = []; z = [1+2i 3-4i]; sg = single([1.5 -2.25]);
        i8 = int8([-128 127]); u64 = uint64(18446744073709551615); i64 = int64(-2^53);
        b = logical([1 0; 0 1]); t = 'İzmir'; cm = ['ab'; 'cd']; ce = '';
        cl = {1, 'iki', {3, {}}}; s = struct('a', {1, 'x'}, 'b', {[], {2}});
        es = struct('p', {}); nd = reshape(1:24, [2 3 4]); cz = cell(2, 0);
        n.ic.deep = 'y'; uzun.alan_adi_otuz_iki_karakterden_cok_daha_uzun_bir_ad = 7;
        names = who();
        for f = {'-v7', '-v6'}
            save('t.mat', f{1}, names{:});
            S = load('t.mat');
            for k = 1:numel(names)
                v = eval(names{k}); w = S.(names{k});
                assert(isequal(v, w) && strcmp(class(v), class(w)) && isequal(size(v), size(w)), names{k});
            end
        end
        save('t4.mat', '-v4', 'd', 'z', 't', 'cm');
        S = load('t4.mat');
        assert(isequal(S.d, d) && isequal(S.z, z) && isequal(S.t, t) && isequal(S.cm, cm));
        A = rand(300); save('big.mat', 'A'); save('bigu.mat', 'A', '-v6');
        S = load('big.mat'); assert(isequal(S.A, A));
        Z = zeros(500); save('z7.mat', 'Z'); save('z6.mat', 'Z', '-v6');
        p = dir('z7.mat'); q = dir('z6.mat'); assert(p.bytes < q.bytes / 50);"#,
    );
}

#[test]
fn struct_option_and_append() {
    run(
        "struct",
        r#"s.a = 1; s.b = 'iki'; s.c = [1 2 3];
        save('s.mat', '-struct', 's');
        S = load('s.mat'); assert(isequal(S, s));
        save('s2.mat', '-struct', 's', 'c', 'a');
        S = load('s2.mat'); assert(isequal(fieldnames(S), {'c'; 'a'}));
        clear all
        load s2
        assert(isequal(c, [1 2 3]) && a == 1);
        x = 1; y = 2;
        save('ap.mat', 'x', 'y');
        x = 10; z = 3;
        save('ap.mat', 'x', 'z', '-append');
        S = load('ap.mat');
        assert(isequal(fieldnames(S), {'x'; 'y'; 'z'}) && S.x == 10 && S.y == 2 && S.z == 3);
        w = 4; save('ap.mat', 'w', '-append', '-v6');
        S = load('ap.mat'); assert(S.w == 4 && S.x == 10);
        save('yeni.mat', 'w', '-append'); S = load('yeni.mat'); assert(S.w == 4);
        failed = false; try, save('s3.mat', '-struct', 'x'); catch, failed = true; end; assert(failed);
        failed = false; try, save('s3.mat', '-struct', 's', 'yok'); catch, failed = true; end; assert(failed);"#,
    );
}

#[test]
fn ascii_save_and_load() {
    let d = temp_dir("ascii");
    run_in(
        &d,
        r#"A = [1 -2.5; 3 4e10]; save('a.txt', 'A', '-ascii');
        B = load('a.txt'); assert(isequal(B, A));
        clear B; load a.txt; assert(isequal(a, A));
        save('d.txt', 'A', '-ascii', '-double', '-tabs');
        C = load('d.txt', '-ascii'); assert(isequal(C, A));
        save -ascii twice.txt A
        save -ascii -append twice.txt A
        assert(isequal(load('twice.txt'), [A; A]));
        fid = fopen('veri.dat', 'w'); fprintf(fid, '%% yorum\n1, 2, 3\n4 5 6\nNaN Inf -Inf\n'); fclose(fid);
        V = load('veri.dat'); assert(isequaln(V, [1 2 3; 4 5 6; NaN Inf -Inf]));
        load('veri.dat'); assert(isequaln(veri, V));
        fid = fopen('kotu.txt', 'w'); fprintf(fid, '1 2\n3\n'); fclose(fid);
        failed = false; try, load kotu.txt; catch e, failed = ~isempty(strfind(e.message, 'columns')); end; assert(failed);
        failed = false; try, save('x.txt', 'A', '-double'); catch, failed = true; end; assert(failed);"#,
    );
    let text = std::fs::read_to_string(d.join("a.txt")).unwrap();
    assert_eq!(
        text,
        "   1.0000000e+00  -2.5000000e+00\n   3.0000000e+00   4.0000000e+10\n"
    );
    let text = std::fs::read_to_string(d.join("d.txt")).unwrap();
    assert_eq!(
        text,
        "1.000000000000000e+00\t-2.500000000000000e+00\n3.000000000000000e+00\t4.000000000000000e+10\n"
    );
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn who_and_whos() {
    run(
        "whos",
        r#"x = magic(3); ad = 'deneme'; z = 1i; global G; G = 5;
        save('w.mat', 'x', 'ad', 'z', 'G');
        c = who('-file', 'w.mat'); assert(isequal(c, {'x'; 'ad'; 'z'; 'G'}));
        s = whos('-file', 'w.mat');
        assert(isequal(size(s), [4 1]) && strcmp(s(1).name, 'x') && isequal(s(1).size, [3 3]));
        assert(s(1).bytes == 72 && strcmp(s(2).class, 'char') && s(3).complex && s(4).global);
        t = evalc('whos -file w.mat');
        assert(~isempty(strfind(t, 'Name')) && ~isempty(strfind(t, '3x3')) && ~isempty(strfind(t, 'global')));
        c = who(); assert(isequal(c, {'G'; 'ad'; 'c'; 's'; 't'; 'x'; 'z'}));
        c = who('a*'); assert(isequal(c, {'ad'}));
        s = whos('x'); assert(strcmp(s.class, 'double') && s.bytes == 72);
        t = evalc('who'); assert(~isempty(strfind(t, 'Your variables are:')));"#,
    );
}

#[test]
fn unsupported_values_warn_and_skip() {
    run(
        "skip",
        r#"f = @(x) x + 1; q = "metin"; k = {1, @sin}; ok = 2; t = table([1;2]);
        lastwarn('');
        save('u.mat');
        [m, id] = lastwarn(); assert(~isempty(m));
        S = load('u.mat');
        assert(isfield(S, 'f') && isfield(S, 'q') && S.ok == 2);
        assert(isfield(S, 'k') && isequal(class(S.k{2}), 'function_handle'));
        lastwarn('');
        S = load('u.mat', 'ok', 'yok');
        [m, id] = lastwarn(); assert(~isempty(strfind(m, 'yok')));
        failed = false; try, save('h.mat', '-v7.3'); catch e, failed = ~isempty(strfind(e.message, '7.3')); end; assert(failed);
        failed = false; try, m = matfile('u.mat'); catch e, failed = true; end; assert(failed);
        failed = false; try, load('yok.mat'); catch e, failed = ~isempty(strfind(e.message, 'yok.mat')); end; assert(failed);
        failed = false; try, save('x.mat', 'yok'); catch e, failed = ~isempty(strfind(e.message, 'yok')); end; assert(failed);"#,
    );
}

#[test]
fn load_in_function_uses_its_workspace() {
    let d = temp_dir("fn");
    std::fs::write(
        d.join("yukle.m"),
        "function r = yukle(f)\n  load(f);\n  r = a + 1;\n  save('ic.mat', 'r');\nend\n",
    )
    .unwrap();
    run_in(
        &d,
        r#"a = 41; save('a.mat', 'a'); clear a
        r = yukle('a.mat'); assert(r == 42 && ~exist('a', 'var'));
        S = load('ic.mat'); assert(isequal(fieldnames(S), {'r'}));"#,
    );
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn reads_octave_files() {
    let x = fixtures();
    let d = temp_dir("octave");
    for f in [
        "vals.m",
        "xc_check.m",
        "oct_v7.mat",
        "oct_v6.mat",
        "oct_sparse_v7.mat",
        "oct_quirk_v6.mat",
        "oct_quirk_v7.mat",
    ] {
        std::fs::copy(x.join(f), d.join(f)).unwrap();
    }
    run_in(
        &d,
        r#"t = evalc('files = {''oct_v7.mat''}; xc_check');
        assert(~isempty(regexp(t, 'mismatches: \n', 'once')), t);
        clear all
        t = evalc('files = {''oct_v6.mat''}; xc_check');
        assert(~isempty(regexp(t, 'mismatches: \n', 'once')), t);
        S = load('oct_sparse_v7.mat');
        assert(issparse(S.sp) && isequal(S.sp, [5 0 0; 0 0 0; 0 7 0]));
        assert(issparse(S.spc) && isequal(S.spc, [1+1i 0; 2 0]));
        assert(issparse(S.spl) && islogical(S.spl) && isequal(S.spl, logical(eye(2))));
        assert(isequal(S.u, 'İzmir ğüş') && isequal(S.cm, ['ab'; 'cd']) && isequal(S.cl{3}, {3}));
        for f = {'oct_quirk_v6.mat', 'oct_quirk_v7.mat'}
            S = load(f{1});
            assert(isequal(S.x, {['ab'; 'cd'], struct('q', ['ef'; 'gh'])}) && isequal(S.y, 'zz'));
        end"#,
    );
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn damaged_files_never_panic() {
    let d = temp_dir("damaged");
    let good = std::fs::read(fixtures().join("oct_v7.mat")).unwrap();
    let mut x: u32 = 7;
    for round in 0..200 {
        let mut b = good.clone();
        for _ in 0..1 + round % 5 {
            x = x.wrapping_mul(1103515245).wrapping_add(12345);
            let i = (x as usize >> 8) % b.len();
            b[i] = (x >> 3) as u8;
        }
        if round % 7 == 0 {
            b.truncate((x as usize) % b.len());
        }
        std::fs::write(d.join("bad.mat"), &b).unwrap();
        let mut it = Interp::new();
        it.set_cwd(d.clone());
        let _ = it
            .run_text("try, S = load('bad.mat'); catch, end; try, whos -file bad.mat; catch, end");
    }
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn sparse_survives_save_and_load() {
    // The bug: `save` wrote an empty double without saying so.
    run(
        "sparse",
        r#"S = sparse([1 2],[1 2],[3 4]); save('sp.mat','S'); clear S; load('sp.mat');
        assert(strcmp(class(S), 'double') && issparse(S) && isequal(size(S), [2 2]));
        assert(nnz(S) == 2 && isequal(full(S), [3 0; 0 4]));
        C = sparse([1 2],[2 2],[1+2i -3i],3,3); L = sparse(logical([0 1; 0 0]));
        E = sparse(2,3); N = sparse(0,0); R = sparse([0 2 0 3]);
        B = sparse(1000000,1000000); B(5,7) = 2;
        for opt = {'-v7','-v6'}
            save('all.mat', opt{1}, 'S','C','L','E','N','R','B');
            X = load('all.mat');
            assert(issparse(X.S) && isequal(X.S, S));
            assert(issparse(X.C) && ~isreal(X.C) && isequal(X.C, C));
            assert(issparse(X.L) && islogical(X.L) && isequal(X.L, L));
            assert(issparse(X.E) && isequal(size(X.E), [2 3]) && nnz(X.E) == 0);
            assert(issparse(X.N) && isequal(size(X.N), [0 0]));
            assert(issparse(X.R) && isequal(X.R, R));
            assert(issparse(X.B) && nnz(X.B) == 1 && X.B(5,7) == 2 && isequal(size(X.B), [1000000 1000000]));
        end
        c = {S, {L}}; s = struct('a', C, 'b', {{E}}); q = struct('v', {S, R});
        save('nest.mat', 'c', 's', 'q'); X = load('nest.mat');
        assert(issparse(X.c{1}) && isequal(X.c{1}, S) && issparse(X.c{2}{1}) && islogical(X.c{2}{1}));
        assert(issparse(X.s.a) && isequal(X.s.a, C) && issparse(X.s.b{1}));
        assert(issparse(X.q(2).v) && isequal(X.q(2).v, R));
        save('st.mat', '-struct', 's', 'a'); X = load('st.mat');
        assert(issparse(X.a) && isequal(X.a, C));
        global G; G = S; save('g.mat', 'G'); clear global G; load('g.mat');
        assert(issparse(G));
        save('app.mat', 'S'); save('app.mat', 'R', '-append'); S2 = S * 2; S = S2; save('app.mat', 'S', '-append');
        X = load('app.mat'); assert(issparse(X.S) && isequal(X.S, S2) && issparse(X.R));"#,
    );
}

#[test]
fn sparse_in_level4_and_ascii() {
    run(
        "sparse4",
        r#"S = sparse([1 2],[1 2],[3 4]); C = sparse([2],[3],[1+2i],3,3); E = sparse(2,3); L = sparse(true);
        save -v4 v4.mat S C E L
        X = load('v4.mat');
        assert(issparse(X.S) && isequal(X.S, S));
        assert(issparse(X.C) && isequal(X.C, C) && isequal(size(X.C), [3 3]));
        assert(issparse(X.E) && isequal(size(X.E), [2 3]) && nnz(X.E) == 0);
        assert(issparse(X.L) && strcmp(class(X.L), 'double') && X.L == 1);
        z = 5; save -v4 v4.mat z -append; X = load('v4.mat');
        assert(issparse(X.S) && issparse(X.C) && X.z == 5);
        save -ascii a.txt S
        assert(isequal(load('a.txt'), [3 0; 0 4]));
        H = sparse(2^20, 2^20);
        t = evalc('save -ascii b.txt H');
        assert(~isempty(strfind(lastwarn(), 'too large to write as text')));"#,
    );
}

#[test]
fn whos_reports_sparse_storage() {
    run(
        "sparsewhos",
        r#"S = speye(3); C = sparse(1i); L = sparse(true); E = sparse(2, 2);
        t = evalc('whos S'); assert(~isempty(strfind(t, 'sparse')));
        w = whos('S'); assert(w.sparse && ~w.complex && w.bytes == 3*16 + 4*8);
        w = whos('C'); assert(w.sparse && w.complex && w.bytes == 24 + 2*8);
        w = whos('L'); assert(w.sparse && ~w.complex && w.bytes == 9 + 2*8);
        w = whos('E'); assert(w.sparse && w.bytes == 16 + 3*8);
        save w.mat S C L
        w = whos('-file', 'w.mat');
        assert(all([w.sparse]) && isequal({w.name}, {'S','C','L'}) && w(2).complex);"#,
    );
}

#[test]
fn reads_octave_sparse_files() {
    let x = fixtures();
    let d = temp_dir("octave_sparse");
    for f in [
        "sparse_vals.m",
        "sparse_check.m",
        "sparse_check4.m",
        "sparse_same.m",
        "oct_sp_v7.mat",
        "oct_sp_v4.mat",
        "oct_sp_small_v6.mat",
    ] {
        std::fs::copy(x.join(f), d.join(f)).unwrap();
    }
    run_in(
        &d,
        r#"t = evalc('files = {''oct_sp_v7.mat''}; sparse_check');
        assert(~isempty(regexp(t, '23 variables, mismatches: \n', 'once')), t);
        t = evalc('files = {''oct_sp_v4.mat''}; sparse_check4');
        assert(~isempty(regexp(t, '15 variables, mismatches: \n', 'once')), t);
        t = evalc('files = {''oct_sp_small_v6.mat''}; sparse_check4');
        assert(~isempty(regexp(t, '20 variables, mismatches: \n', 'once')), t);
        % Octave stores logical sparse as uint8 with the logical flag; these
        % two were misread: as many row indices as elements, and no elements.
        S = load('oct_sp_v7.mat');
        assert(issparse(S.sp_logfull) && islogical(S.sp_logfull) && isequal(S.sp_logfull, [true true]));
        assert(issparse(S.sp_log00) && islogical(S.sp_log00) && isequal(size(S.sp_log00), [0 0]));"#,
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// Variables of `file` after `load`, and the warnings `code` printed.
fn saved_names(d: &Path, setup_and_save: &str) -> (String, String) {
    let src = format!(
        "{setup_and_save}\n__w = lastwarn(); __n = strjoin(sort(who('-file', 'o.mat'))', ',');"
    );
    let mut it = Interp::new();
    it.set_cwd(d.to_path_buf());
    if let Err(e) = it.run_text(&src) {
        panic!("{}\n  -> {}", src, e.message);
    }
    (
        it.get_var("__n").unwrap().to_str().unwrap(),
        it.get_var("__w").unwrap().to_str().unwrap(),
    )
}

#[test]
fn unsupported_values_are_skipped_with_a_warning() {
    let d = temp_dir("skipped");
    std::fs::write(
        d.join("Pt.m"),
        "classdef Pt\n properties\n  v\n end\n methods\n  function o = Pt(x), if nargin, o.v = x; end, end\n end\nend\n",
    )
    .unwrap();
    std::fs::write(
        d.join("Hd.m"),
        "classdef Hd < handle\n properties\n  v\n end\nend\n",
    )
    .unwrap();
    let values = [
        ("table([1;2])", "table objects cannot be saved"),
        ("datetime(2020,1,1)+(0:2)", "class 'datetime'"),
        ("categorical({'a','b'})", "class 'categorical'"),
        ("hours(1:2)", "class 'duration'"),
        ("[Pt(1) Pt(2)]", "class 'Pt'"),
        ("Pt(1)", "class 'Pt'"),
        ("Hd()", "class 'Hd'"),
        ("[Hd() Hd()]", "class 'Hd'"),
    ];
    for (expr, why) in values {
        for fmt in ["-v7", "-v6"] {
            // As a variable of its own, nothing can stand in for it.
            let (names, warning) = saved_names(
                &d,
                &format!("ok = 1; x = {expr}; save('o.mat', '{fmt}', 'ok', 'x');"),
            );
            assert_eq!(names, "ok", "{expr} {fmt}");
            assert!(warning.contains("'x' was not saved"), "{expr}: {warning}");
            assert!(warning.contains(why), "{expr}: {warning}");
            // In a container that element is empty, the warning names it, and
            // the rest of the variable is kept.
            let (names, warning) = saved_names(
                &d,
                &format!(
                    "ok = 1; c = {{1, {{2, {expr}}}}}; save('o.mat', '{fmt}', 'ok', 'c'); S = load('o.mat'); assert(isequal(S.c, {{1, {{2, []}}}}));"
                ),
            );
            assert_eq!(names, "c,ok", "cell {expr} {fmt}");
            assert!(
                warning.contains("'c{2}{2}' was saved as []"),
                "{expr}: {warning}"
            );
            assert!(warning.contains(why), "{expr}: {warning}");
            let (names, warning) = saved_names(
                &d,
                &format!(
                    "ok = 1; s = struct('n', 5); s(2).n = 6; s(2).f = {{1, {expr}}}; save('o.mat', '{fmt}', 'ok', 's'); S = load('o.mat'); assert(S.s(1).n == 5 && S.s(2).n == 6 && isequal(S.s(2).f, {{1, []}}));"
                ),
            );
            assert_eq!(names, "ok,s", "struct {expr} {fmt}");
            assert!(
                warning.contains("'s(2).f{2}' was saved as []"),
                "{expr}: {warning}"
            );
        }
        let (names, warning) = saved_names(
            &d,
            &format!("ok = 1; x = {expr}; save('o.mat', '-v4', 'ok', 'x');"),
        );
        assert_eq!(names, "ok", "{expr} -v4");
        assert!(warning.contains("'x' was not saved"), "{expr}: {warning}");
    }
    // -struct: an unsupported field is skipped, the others are written.
    let (names, warning) = saved_names(
        &d,
        "s.a = 1; s.t = table([1;2]); save('o.mat', '-struct', 's');",
    );
    assert_eq!(names, "a");
    assert!(warning.contains("'t' was not saved"), "{warning}");
    let _ = std::fs::remove_dir_all(&d);
}

// ---------------------------------------------------------------------------
// Function handles and string arrays
// ---------------------------------------------------------------------------

#[test]
fn function_handles_round_trip() {
    for fmt in ["-v7", "-v6"] {
        run(
            "handles",
            &format!(
                r#"a = 3; b = [1 2; 3 4]; name = 'adı'; w = {{2, 'x'}};
                n = @sin; f = @(x) x + a; g = @(x, y) a * x + y(end); h = @() numel(name);
                m = @(x) b(x, 2); q = @(x) w{{1}} * x; plain = @(x) x .^ 2; dotted = @strrep;
                ic = @(x) @(y) x + y + a; many = @(varargin) numel(varargin);
                save('h.mat', '{fmt}');
                clear -regexp ^[a-z]
                load('h.mat');
                assert(strcmp(class(n), 'function_handle') && n(0) == 0);
                assert(strcmp(func2str(n), 'sin'));
                assert(f(2) == 5 && g(2, [9 1]) == 7 && h() == 3);
                assert(strcmp(func2str(f), '@(x)x+a'));
                assert(m(2) == 4 && q(4) == 8 && plain(3) == 9);
                assert(strcmp(dotted('aa', 'a', 'b'), 'bb'));
                k = ic(1); assert(k(1) == 5);
                assert(many(1, 2, 3) == 3);
                s = functions(f); assert(s.workspace{{1}}.a == 3);"#
            ),
        );
    }
}

#[test]
fn handles_inside_containers_and_mixed_files() {
    run(
        "handles_mixed",
        r#"a = 2; t = "metin";
        c = {1, @cos, {@(x) x * a, "iç"}}; s = struct('f', @(y) y - a, 'n', 5, 'g', {{@sin}});
        arr = struct('h', {@tan, @(z) z + 1});
        w = @(x) c{2}(x) + a; d = magic(3);
        save('m.mat');
        S = load('m.mat');
        assert(S.c{2}(0) == 1 && S.c{3}{1}(4) == 8 && strcmp(S.c{3}{2}, "iç"));
        assert(S.s.f(5) == 3 && S.s.n == 5 && S.s.g{1}(0) == 0);
        assert(S.arr(1).h(0) == 0 && S.arr(2).h(1) == 2);
        assert(S.w(0) == 3 && isequal(S.d, d) && strcmp(S.t, "metin"));
        names = who('-file', 'm.mat');
        assert(any(strcmp(names, 'w')) && numel(names) == 7);
        info = whos('-file', 'm.mat');
        assert(strcmp(info(strcmp({info.name}, 'w')).class, 'function_handle'));"#,
    );
}

#[test]
fn string_arrays_round_trip() {
    for fmt in ["-v7", "-v6"] {
        run(
            "strings",
            &format!(
                r#"a = "metin"; b = ["bir" "iki"; "üç" "İzmir"]; e = ""; m = string(missing);
                z = strings(0, 3); nd = reshape(string(1:8), 2, 2, 2); c = {{"x", 1}}; s = struct('t', "y");
                u = "𝄞 nota";
                A = a; B = b; Z = size(z); ND = nd; U = u;
                save('s.mat', '{fmt}', 'a', 'b', 'e', 'm', 'z', 'nd', 'c', 's', 'u');
                clear a b e m z nd c s u
                load('s.mat');
                assert(isstring(a) && a == A);
                assert(isstring(b) && isequal(b, B));
                assert(isstring(e) && strlength(e) == 0 && ~ismissing(e));
                assert(isstring(m) && ismissing(m));
                assert(isstring(z) && isequal(size(z), Z));
                assert(isequal(nd, ND) && isequal(size(nd), [2 2 2]));
                assert(isstring(c{{1}}) && c{{1}} == "x" && s.t == "y");
                assert(u == U);"#
            ),
        );
    }
}

#[test]
fn local_and_nested_handles() {
    let d = temp_dir("local");
    std::fs::write(
        d.join("yerel.m"),
        "function [l, a, n] = yerel()\n  l = @ic;\n  a = @(x) ic(x) + 1;\n  n = @dis;\nend\nfunction r = ic(x)\n  r = 2 * x;\nend\n",
    )
    .unwrap();
    std::fs::write(
        d.join("yuva.m"),
        "function h = yuva()\n  t = 1;\n  h = @sayac;\n  function r = sayac()\n    t = t + 1;\n    r = t;\n  end\nend\n",
    )
    .unwrap();
    std::fs::write(
        d.join("dis.m"),
        "function r = dis(x)\n  r = x + 100;\nend\n",
    )
    .unwrap();
    // A local function comes back through its file.
    run_in(
        &d,
        r#"[l, a, n] = yerel();
        lastwarn('');
        save('l.mat', 'l', 'n');
        assert(isempty(lastwarn()));
        S = load('l.mat');
        assert(S.l(4) == 8 && S.n(1) == 101);"#,
    );
    // The text is all an anonymous function keeps: say so before it fails.
    let (names, warning) = saved_names(&d, "[l, a] = yerel(); save('o.mat', 'a');");
    assert_eq!(names, "a");
    assert!(warning.contains("calls local function 'ic'"), "{warning}");
    // A nested function's workspace cannot be restored.
    let (names, warning) = saved_names(&d, "h = yuva(); ok = 1; save('o.mat', 'h', 'ok');");
    assert_eq!(names, "ok");
    assert!(warning.contains("'h' was not saved"), "{warning}");
    assert!(warning.contains("nested function 'sayac'"), "{warning}");
    let (names, warning) = saved_names(
        &d,
        "h = yuva(); c = {1, h}; save('o.mat', 'c'); S = load('o.mat'); assert(isequal(S.c, {1, []}));",
    );
    assert_eq!(names, "c");
    assert!(warning.contains("'c{2}' was saved as []"), "{warning}");
    // A handle capturing one is not stored in part: it would fail when called.
    let (names, warning) = saved_names(
        &d,
        "h = yuva(); f = @() h() + 1; t = table([1;2]); g = @(i) t.Var1(i); ok = 1; save('o.mat', 'f', 'g', 'ok');",
    );
    assert_eq!(names, "ok");
    assert!(warning.contains("captures 't'"), "{warning}");
    // The file of a local function is gone: the load says so.
    run_in(
        &d,
        r#"[l, a] = yerel(); save('g.mat', 'l'); movefile('yerel.m', 'yerel.txt');
        lastwarn('');
        S = load('g.mat');
        assert(~isfield(S, 'l'));
        assert(contains(lastwarn(), 'local function'));
        movefile('yerel.txt', 'yerel.m');"#,
    );
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn append_keeps_objects_valid() {
    run(
        "append_obj",
        r#"a = 1; f = @(x) x + a; save('a.mat', 'f');
        % A plain variable goes before the object data, which stays last.
        b = 2; save('a.mat', 'b', '-append');
        S = load('a.mat'); assert(S.f(1) == 2 && S.b == 2);
        % Adding objects to a file with objects renumbers them.
        g = @(x) x * 10; t = "s"; save('a.mat', 'g', 't', '-append');
        S = load('a.mat'); assert(S.f(1) == 2 && S.g(2) == 20 && S.t == "s" && S.b == 2);
        % Replacing, with and without objects.
        f = @(x) x - a; save('a.mat', 'f', '-append');
        S = load('a.mat'); assert(S.f(1) == 0 && S.g(2) == 20);
        f = 5; save('a.mat', 'f', '-append');
        S = load('a.mat'); assert(S.f == 5 && S.g(2) == 20 && S.t == "s");
        % Objects into a file without any.
        p = 1; save('p.mat', 'p'); q = @cos; r = @(x) x + p; save('p.mat', 'r', 'q', '-append');
        S = load('p.mat'); assert(S.p == 1 && S.q(0) == 1 && S.r(1) == 2);
        % -append -v6 keeps working the same way.
        save('v.mat', '-v6', 'f'); save('v.mat', '-v6', 'r', '-append');
        S = load('v.mat'); assert(S.f == 5 && S.r(1) == 2);"#,
    );
}

/// The saved file with its object metadata version changed: the reader then
/// knows there is object data, but not what it says.
fn break_object_metadata(path: &Path) {
    let mut file = std::fs::read(path).unwrap();
    let names = file
        .windows(4)
        .rposition(|w| w == b"any\0")
        .expect("the file holds no object metadata");
    file[names - 40] = 9;
    std::fs::write(path, file).unwrap();
}

#[test]
fn append_refuses_new_objects_next_to_objects_it_cannot_read() {
    let d = temp_dir("append_unreadable");
    run_in(
        &d,
        r#"f = @(x) x + 1; p = 7; save('a.mat', '-v6', 'f', 'p');"#,
    );
    let path = d.join("a.mat");
    break_object_metadata(&path);
    let before = std::fs::read(&path).unwrap();
    run_in(
        &d,
        r#"s = "text"; g = @(x) x;
        for v = {'s', 'g'}
          failed = false;
          try, save('a.mat', v{1}, '-append'); catch e
            failed = true; assert(strcmp(e.identifier, 'IndyR:save:appendObjects'));
          end
          assert(failed);
        end
        n = 2; save('a.mat', 'n', '-append');
        S = load('a.mat', 'n', 'p'); assert(S.n == 2 && S.p == 7);"#,
    );
    let after = std::fs::read(&path).unwrap();
    assert_eq!(&after[..before.len()], &before[..], "the old bytes stay");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_captured_value_that_cannot_load_does_not_stop_the_file() {
    let d = temp_dir("captured_unloadable");
    run_in(
        &d,
        r#"s = "text"; f = @(x) x + 1 + numel(s); p = 7; save('a.mat', '-v6', 'f', 'p');"#,
    );
    // The captured string becomes an object of a class the engine cannot load.
    let path = d.join("a.mat");
    let mut file = std::fs::read(&path).unwrap();
    let mut at = 0;
    while let Some(k) = file[at..].windows(6).position(|w| w == b"string") {
        file[at + k..at + k + 6].copy_from_slice(b"strinG");
        at += k + 6;
    }
    assert!(at > 0, "no captured string in the file");
    std::fs::write(&path, file).unwrap();
    run_in(
        &d,
        r#"S = load('a.mat'); assert(S.p == 7); assert(isa(S.f, 'function_handle'));
        assert(S.f(1) == 2);"#,
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// Files MATLAB R2025b wrote, shipped with Octave's own tests: not committed
/// here (they belong to its GPL distribution), used when installed.
fn matlab_samples() -> Option<PathBuf> {
    let base = Path::new("/opt/homebrew/Cellar/octave");
    let version = std::fs::read_dir(base).ok()?.flatten().next()?.path();
    let share = version.join("share/octave");
    let inner = std::fs::read_dir(share).ok()?.flatten().find(|e| {
        e.file_name()
            .to_str()
            .is_some_and(|n| n.starts_with(|c: char| c.is_ascii_digit()))
    })?;
    let dir = inner.path().join("etc/tests/fixed/bug-67964");
    dir.join("foo1.mat").is_file().then_some(dir)
}

/// The elements of a level 5 file, inflated.
fn elements(file: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut p = 128;
    while p + 8 <= file.len() {
        let w = |at: usize| u32::from_le_bytes(file[at..at + 4].try_into().unwrap());
        let (ty, size) = (w(p), w(p + 4) as usize);
        let end = p + 8 + size;
        out.push(if ty == 15 {
            indy_interp::builtins::matfile::zlib::inflate(&file[p + 8..end], 1 << 30).unwrap()
        } else {
            file[p..end].to_vec()
        });
        p = end;
    }
    out
}

#[test]
fn reads_matlab_handles_and_writes_the_same_bytes() {
    let Some(dir) = matlab_samples() else {
        eprintln!("skipped: MATLAB's sample files (Octave 11's bug-67964 test data) are not here");
        return;
    };
    let d = temp_dir("matlab_bytes");
    let shown = dir.display();
    run_in(
        &d,
        &format!(
            r#"S = load('{shown}/foo1.mat'); assert(strcmp(func2str(S.f), '@(x)x+1') && S.f(2) == 3);
            S = load('{shown}/foo2.mat'); assert(strcmp(func2str(S.g), '@(x)x+a') && S.g(2) == 5);
            f = @(x) x + 1; save('f.mat', '-v6', 'f');
            a = 3; g = @(x) x + a; save('g.mat', '-v6', 'g');"#
        ),
    );
    for (mine, theirs) in [("f.mat", "foo1.mat"), ("g.mat", "foo2.mat")] {
        let a = elements(&std::fs::read(d.join(mine)).unwrap());
        let b = elements(&std::fs::read(dir.join(theirs)).unwrap());
        assert_eq!(a.len(), 2);
        assert_eq!(b.len(), 2);
        // The object data is identical byte for byte.
        assert_eq!(a[1], b[1], "object data of {mine}");
        // The variable differs only in `matlabroot` and the separator (one
        // sample is from Windows), and in the sizes enclosing them.
        let after_separator = |e: &[u8]| {
            let at = e
                .windows(16)
                .position(|w| w == b"function_handle\0")
                .unwrap();
            let mut p = at + e[at..].windows(4).position(|w| w == [14, 0, 0, 0]).unwrap();
            for _ in 0..2 {
                let size = u32::from_le_bytes(e[p + 4..p + 8].try_into().unwrap());
                p += 8 + size as usize;
            }
            e[p..].to_vec()
        };
        assert_eq!(after_separator(&a[0]), after_separator(&b[0]), "{mine}");
    }
    let _ = std::fs::remove_dir_all(&d);
}
