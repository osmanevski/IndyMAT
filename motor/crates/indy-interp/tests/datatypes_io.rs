//! Delimited text I/O (readtable, writetable, readmatrix, writematrix,
//! readcell, writecell, readlines, writelines, readvars, textscan,
//! detectImportOptions) through the interpreter.

use std::path::PathBuf;

use indy_interp::Interp;

/// Interpreter whose cwd is a fresh per-test directory.
fn interp(tag: &str) -> Interp {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("datatypes_io_{}", tag));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut it = Interp::new();
    it.set_cwd(dir);
    it
}

fn check(it: &mut Interp, cond: &str) {
    let code = format!("t__ = isequal({}, true);", cond);
    if let Err(e) = it.run_text(&code) {
        panic!("{} -> error: {}", cond, e.message);
    }
    let ok = it.get_var("t__").and_then(|v| v.scalar_f64()) == Some(1.0);
    assert!(ok, "failed: {}", cond);
}

fn run(it: &mut Interp, code: &str) {
    if let Err(e) = it.run_text(code) {
        panic!("{} -> error: {}", code, e.message);
    }
}

fn fails_with(it: &mut Interp, code: &str, needle: &str) {
    match it.run_text(code) {
        Ok(_) => panic!("expected an error: {}", code),
        Err(e) => assert!(
            e.message.contains(needle),
            "error for {} was '{}', expected '{}'",
            code,
            e.message,
            needle
        ),
    }
}

fn write(it: &mut Interp, name: &str, bytes: &[u8]) {
    let p = it.cwd.join(name);
    std::fs::write(p, bytes).unwrap();
}

#[test]
fn table_round_trip() {
    let mut it = interp("table_round_trip");
    run(
        &mut it,
        "T = table([1; 2], [0.5; 1.5], {'a'; 'b, c'}, [true; false], 'VariableNames', {'n', 'x', 's', 'b'});
         writetable(T, 't.csv'); R = readtable('t.csv'); txt = fileread('t.csv');",
    );
    check(&mut it, "isequal(R.n, [1; 2])");
    check(&mut it, "isequal(R.x, [0.5; 1.5])");
    check(&mut it, "isequal(R.s, {'a'; 'b, c'})");
    check(&mut it, "isequal(R.b, [1; 0])");
    check(
        &mut it,
        "isequal(R.Properties.VariableNames, {'n', 'x', 's', 'b'})",
    );
    check(
        &mut it,
        "strcmp(txt, sprintf('n,x,s,b\\n1,0.5,a,1\\n2,1.5,\"b, c\",0\\n'))",
    );
}

#[test]
fn writetable_precision_and_options() {
    let mut it = interp("writetable_options");
    run(
        &mut it,
        "T = table([pi; NaN], {'q\"t'; 'z'}, 'VariableNames', {'v', 'w'});
         writetable(T, 'a.txt', 'Delimiter', ';', 'QuoteStrings', true);
         a = fileread('a.txt');
         writetable(T, 'b.csv', 'WriteVariableNames', false);
         writetable(T, 'b.csv', 'WriteMode', 'append');
         b = fileread('b.csv');",
    );
    check(
        &mut it,
        "strcmp(a, sprintf('v;w\\n3.14159265358979;\"q\"\"t\"\\nNaN;\"z\"\\n'))",
    );
    check(
        &mut it,
        "strcmp(b, sprintf('3.14159265358979,\"q\"\"t\"\\nNaN,z\\n3.14159265358979,\"q\"\"t\"\\nNaN,z\\n'))",
    );
    run(
        &mut it,
        "T = table([1 2; 3 4], 'VariableNames', {'m'}); writetable(T, 'm.csv'); m = fileread('m.csv');",
    );
    check(&mut it, "strcmp(m, sprintf('m_1,m_2\\n1,2\\n3,4\\n'))");
}

#[test]
fn readtable_quotes_header_names() {
    let mut it = interp("readtable_quotes");
    write(
        &mut it,
        "q.csv",
        b"Name,Age,my score,for\n\"Smith, J\",30,1.5,a\nLee,,2e3,\"say \"\"hi\"\"\"\n",
    );
    run(&mut it, "T = readtable('q.csv');");
    check(
        &mut it,
        "isequal(T.Properties.VariableNames, {'Name', 'Age', 'myScore', 'xFor'})",
    );
    check(&mut it, "isequal(T.Name, {'Smith, J'; 'Lee'})");
    check(&mut it, "isequaln(T.Age, [30; NaN])");
    check(&mut it, "isequal(T.myScore, [1.5; 2000])");
    check(&mut it, "isequal(T.xFor, {'a'; 'say \"hi\"'})");
    check(
        &mut it,
        "strcmp(T.Properties.VariableDescriptions{3}, 'Original column heading: ''my score''')",
    );
    run(
        &mut it,
        "T = readtable('q.csv', 'ReadVariableNames', false);",
    );
    check(
        &mut it,
        "isequal(size(T), [3 4]) && isequal(T.Var1, {'Name'; 'Smith, J'; 'Lee'})",
    );
    run(&mut it, "T = readtable('q.csv', 'TextType', 'string');");
    check(&mut it, "isstring(T.Name)");
    run(
        &mut it,
        "T = readtable('q.csv', 'VariableNamingRule', 'preserve');",
    );
    check(&mut it, "strcmp(T.Properties.VariableNames{3}, 'my score')");
}

#[test]
fn readtable_bom_delimiters_headerlines() {
    let mut it = interp("readtable_delims");
    let mut b = vec![0xEF, 0xBB, 0xBF];
    b.extend_from_slice("şehir;nüfus\nİzmir;4,5\nAnkara;5\n".as_bytes());
    write(&mut it, "s.csv", &b);
    run(&mut it, "T = readtable('s.csv');");
    // ';' is detected; "4,5" stays text so the column is text
    check(
        &mut it,
        "isequal(T.Properties.VariableNames, {'x_ehir', 'n_fus'})",
    );
    check(&mut it, "isequal(T.x_ehir, {'İzmir'; 'Ankara'})");
    check(&mut it, "isequal(T.n_fus, {'4,5'; '5'})");
    write(&mut it, "t.txt", b"a\tb\n1\t2\n3\t4\n");
    run(
        &mut it,
        "T = readtable('t.txt'); [p, q] = readvars('t.txt');",
    );
    check(
        &mut it,
        "isequal(T.b, [2; 4]) && isequal(p, [1; 3]) && isequal(q, [2; 4])",
    );
    write(&mut it, "h.dat", b"comment line\nx y\n 1  2\n3   4\n");
    run(&mut it, "T = readtable('h.dat', 'NumHeaderLines', 1);");
    check(&mut it, "isequal(T.x, [1; 3]) && isequal(T.y, [2; 4])");
    run(
        &mut it,
        "T = readtable('h.dat', 'NumHeaderLines', 2, 'ReadVariableNames', false, 'VariableNames', {'p', 'q'});",
    );
    check(&mut it, "isequal(T.p, [1; 3]) && isequal(T.q, [2; 4])");
    write(&mut it, "r.csv", b"id,v\nr1,1\nr2,2\n");
    run(&mut it, "T = readtable('r.csv', 'ReadRowNames', true);");
    check(
        &mut it,
        "isequal(T.Properties.RowNames, {'r1'; 'r2'}) && isequal(T.v, [1; 2])",
    );
}

#[test]
fn detect_import_options_round_trip() {
    let mut it = interp("detect_opts");
    write(&mut it, "d.csv", b"a,b,c\n1,x,3\n4,y,6\n");
    run(&mut it, "o = detectImportOptions('d.csv');");
    check(&mut it, "isequal(o.VariableNames, {'a', 'b', 'c'})");
    check(
        &mut it,
        "isequal(o.VariableTypes, {'double', 'char', 'double'})",
    );
    check(
        &mut it,
        "isequal(o.Delimiter, {','}) && isequal(o.DataLines, [2 Inf])",
    );
    run(
        &mut it,
        "o.SelectedVariableNames = {'c', 'a'}; T = readtable('d.csv', o);",
    );
    check(
        &mut it,
        "isequal(T.Properties.VariableNames, {'c', 'a'}) && isequal(T.c, [3; 6])",
    );
    run(
        &mut it,
        "o = setvartype(detectImportOptions('d.csv'), 'a', 'int32'); T = readtable('d.csv', o);",
    );
    check(&mut it, "isa(T.a, 'int32') && isequal(T.a, int32([1; 4]))");
}

#[test]
fn matrix_round_trip() {
    let mut it = interp("matrix");
    run(
        &mut it,
        "A = [NaN Inf -Inf; 0 -1 2.5; 1e20 0.125 -3e-7]; writematrix(A, 'm.csv'); B = readmatrix('m.csv');
         writematrix([1 2], 'e.csv'); writematrix([3 4], 'e.csv', 'WriteMode', 'append'); E = readmatrix('e.csv');
         writematrix(int8([1 -2]), 'i.txt', 'Delimiter', 'tab'); I = fileread('i.txt');
         writematrix([pi 1/3], 'p.csv'); P15 = fileread('p.csv');",
    );
    check(&mut it, "isequaln(A, B)");
    check(&mut it, "isequal(E, [1 2; 3 4])");
    check(&mut it, "strcmp(I, sprintf('1\\t-2\\n'))");
    check(
        &mut it,
        "strcmp(P15, sprintf('3.14159265358979,0.333333333333333\\n'))",
    );
    write(&mut it, "h.csv", b"ad,x,y\nsatir,1,2\nsatir,3,4\n");
    run(
        &mut it,
        "H = readmatrix('h.csv'); H1 = readmatrix('h.csv', 'NumHeaderLines', 1);",
    );
    check(
        &mut it,
        "isequaln(H, [NaN 1 2; NaN 3 4]) && isequaln(H1, H)",
    );
    write(&mut it, "g.csv", b"1,,x\n4,5,6\n");
    run(
        &mut it,
        "G = readmatrix('g.csv'); G32 = readmatrix('g.csv', 'OutputType', 'single');",
    );
    check(
        &mut it,
        "isequaln(G, [1 NaN NaN; 4 5 6]) && isa(G32, 'single')",
    );
    write(&mut it, "w.txt", b"  1   2  3\n 4 5 6\n");
    run(&mut it, "W = readmatrix('w.txt');");
    check(&mut it, "isequal(W, [1 2 3; 4 5 6])");
    write(&mut it, "c.csv", b"1,23;4,56\n7,89;0,12\n");
    run(&mut it, "P = readmatrix('c.csv');");
    check(&mut it, "isequaln(P, [1 NaN 56; 7 NaN 12])");
}

#[test]
fn cell_round_trip() {
    let mut it = interp("cell");
    run(
        &mut it,
        "C = {'a,b', 1; 'c\"d', 2.5}; writecell(C, 'c.csv'); D = readcell('c.csv');
         writecell({'x', true, missing}, 'm.csv'); M = readcell('m.csv'); mt = fileread('m.csv');",
    );
    check(&mut it, "isequal(C, D)");
    check(&mut it, "strcmp(mt, sprintf('x,1,\\n'))");
    check(
        &mut it,
        "strcmp(M{1}, 'x') && isequal(M{2}, 1) && ismissing(M{3})",
    );
    let mut b = vec![0xEF, 0xBB, 0xBF];
    b.extend_from_slice("İzmir,1\nAnkara,2\n".as_bytes());
    write(&mut it, "bom.csv", &b);
    run(&mut it, "B = readcell('bom.csv');");
    check(&mut it, "isequal(B, {'İzmir', 1; 'Ankara', 2})");
    write(&mut it, "s.csv", b"a;1;x\nb;2;y\n");
    run(&mut it, "S = readcell('s.csv');");
    check(&mut it, "isequal(S, {'a', 1, 'x'; 'b', 2, 'y'})");
    write(&mut it, "q.csv", b"\"a,b\";2\n\"x,y\";3\n");
    run(&mut it, "Q = readcell('q.csv');");
    check(&mut it, "isequal(Q, {'a,b', 2; 'x,y', 3})");
}

#[test]
fn lines_round_trip() {
    let mut it = interp("lines");
    run(
        &mut it,
        "writelines({'bir', 'iki'}, 'l.txt'); S = readlines('l.txt');
         writelines(\"uc\", 'l.txt', 'WriteMode', 'append'); K = readlines('l.txt', 'EmptyLineRule', 'skip');",
    );
    check(&mut it, "isequal(S, [\"bir\"; \"iki\"; \"\"])");
    check(&mut it, "isequal(K, [\"bir\"; \"iki\"; \"uc\"])");
    let mut b = vec![0xEF, 0xBB, 0xBF];
    b.extend_from_slice("İzmir\r\nAnkara".as_bytes());
    write(&mut it, "b.txt", &b);
    run(&mut it, "B = readlines('b.txt');");
    check(&mut it, "isequal(B, [\"İzmir\"; \"Ankara\"])");
    write(&mut it, "e.txt", b"");
    run(&mut it, "E = readlines('e.txt');");
    check(&mut it, "isequal(E, \"\")");
    fails_with(
        &mut it,
        "writelines(['a '; 'bb'], 'x.txt');",
        "character vector",
    );
}

#[test]
fn textscan_formats() {
    let mut it = interp("textscan");
    run(&mut it, "c = textscan(sprintf('a 1\\nb 2'), '%s %f');");
    check(
        &mut it,
        "isequal(c{1}, {'a'; 'b'}) && isequal(c{2}, [1; 2])",
    );
    run(
        &mut it,
        "c = textscan(sprintf('1,2,3\\n4,,6'), '%f %f %f', 'Delimiter', ',');",
    );
    check(&mut it, "isequaln([c{:}], [1 2 3; 4 NaN 6])");
    run(
        &mut it,
        "c = textscan(sprintf('h1\\nh2\\nx=1;y=2\\nx=3;y=4'), 'x=%f;y=%f', 'HeaderLines', 2);",
    );
    check(&mut it, "isequal([c{:}], [1 2; 3 4])");
    run(&mut it, "c = textscan('a b,,c', '%s', 'Delimiter', ',');");
    check(&mut it, "isequal(c{1}, {'a b'; ''; 'c'})");
    run(
        &mut it,
        "c = textscan('a,,b', '%s', 'Delimiter', ',', 'MultipleDelimsAsOne', 1);",
    );
    check(&mut it, "isequal(c{1}, {'a'; 'b'})");
    run(
        &mut it,
        "c = textscan('1 2 3 4', '%f %f', 'CollectOutput', 1);",
    );
    check(&mut it, "numel(c) == 1 && isequal(c{1}, [1 2; 3 4])");
    run(&mut it, "c = textscan('1 2 x 4', '%f');");
    check(&mut it, "isequal(c{1}, [1; 2])");
    fails_with(
        &mut it,
        "textscan('1 2 x 4', '%f', 'ReturnOnError', false);",
        "Mismatch",
    );
    run(&mut it, "[c, p] = textscan('10 20 30', '%d', 2);");
    check(
        &mut it,
        "isa(c{1}, 'int32') && isequal(c{1}, int32([10; 20])) && p >= 5 && p <= 6",
    );
    run(
        &mut it,
        "c = textscan('\"a, b\",1', '%q %f', 'Delimiter', ',');",
    );
    check(&mut it, "isequal(c{1}, {'a, b'}) && c{2} == 1");
    run(&mut it, "c = textscan('skip 1 skip 2', '%*s %f');");
    check(&mut it, "numel(c) == 1 && isequal(c{1}, [1; 2])");
    run(&mut it, "c = textscan('ab', '%c');");
    check(&mut it, "ischar(c{1}) && isequal(c{1}, ['a'; 'b'])");
    run(&mut it, "c = textscan('1;2', '%f', 'EndOfLine', ';');");
    check(&mut it, "isequal(c{1}, [1; 2])");
    run(
        &mut it,
        "c = textscan(sprintf('1 2 %% note\\n3 4'), '%f %f', 'CommentStyle', '%');",
    );
    check(&mut it, "isequal([c{:}], [1 2; 3 4])");
    run(&mut it, "c = textscan(sprintf('1 2\\n3 4'), '');");
    check(&mut it, "isequal([c{:}], [1 2; 3 4])");
    run(&mut it, "c = textscan('', '%f %s');");
    check(&mut it, "isempty(c{1}) && iscell(c{2}) && isempty(c{2})");
}

#[test]
fn textscan_file_id() {
    let mut it = interp("textscan_fid");
    write(&mut it, "f.txt", b"x y\n1 2\n3 4\n");
    run(
        &mut it,
        "fid = fopen('f.txt'); c = textscan(fid, '%f %f', 'HeaderLines', 1); fclose(fid);",
    );
    check(&mut it, "isequal([c{:}], [1 2; 3 4])");
    run(
        &mut it,
        "fid = fopen('f.txt'); h = fgetl(fid); c = textscan(fid, '%f %f', 1); d = textscan(fid, '%f %f'); fclose(fid);",
    );
    check(
        &mut it,
        "strcmp(h, 'x y') && isequal([c{:}], [1 2]) && isequal([d{:}], [3 4])",
    );
    fails_with(&mut it, "textscan(99, '%f');", "file identifier");
}

#[test]
fn unsupported_and_errors() {
    let mut it = interp("errors");
    run(&mut it, "T = table([1; 2], 'VariableNames', {'n'});");
    fails_with(
        &mut it,
        "writetable(T, 'x.xls');",
        "writetable: spreadsheet files (.xls) are not supported yet; use .csv or .txt",
    );
    fails_with(
        &mut it,
        "readtable('x.xls');",
        "readtable: spreadsheet files (.xls) are not supported yet; use .csv or .txt",
    );
    fails_with(&mut it, "readmatrix('x.xls');", "spreadsheet files (.xls)");
    fails_with(&mut it, "readcell('x.ods');", "spreadsheet files (.ods)");
    fails_with(
        &mut it,
        "readtable('a.csv', 'FileType', 'spreadsheet');",
        "spreadsheet",
    );
    fails_with(
        &mut it,
        "readtable('missing_file.csv');",
        "Unable to find or open",
    );
    write(&mut it, "a.csv", b"a\n1\n");
    fails_with(
        &mut it,
        "readtable('a.csv', 'Bogus', 1);",
        "unsupported option",
    );
    fails_with(&mut it, "readtable('a.csv', 'Delimiter');", "pairs");
    fails_with(&mut it, "writematrix({1}, 'c.csv');", "writecell");
    fails_with(&mut it, "writecell({struct()}, 'c.csv');", "writecell");
    fails_with(&mut it, "textscan('1', '%D');", "not supported");
    fails_with(&mut it, "writetable(1, 'a.csv');", "table");
}

#[test]
fn odd_inputs_do_not_panic() {
    let mut it = interp("odd");
    write(&mut it, "u.csv", b"\"unterminated,1\n2,3");
    write(&mut it, "z.csv", b"");
    write(&mut it, "r.csv", b"a,b\n1\n1,2,3\n\n\n");
    write(&mut it, "bin.csv", &[0xff, 0xfe, 0x00, 0xd8, 0x41, 0x00]);
    write(&mut it, "lat.csv", &[0x61, 0xe9, 0x2c, 0x31, 0x0a]);
    for code in [
        "x = readtable('u.csv');",
        "x = readcell('u.csv');",
        "x = readmatrix('u.csv');",
        "x = readtable('z.csv');",
        "x = readmatrix('z.csv');",
        "x = readcell('z.csv');",
        "x = readlines('z.csv');",
        "x = readtable('r.csv');",
        "x = readtable('bin.csv');",
        "x = readcell('lat.csv');",
        "x = textscan('', '');",
        "x = textscan(',,,', '%s', 'Delimiter', ',');",
        "x = textscan('abc', '%[a]');",
        "x = textscan('abc', 'zz');",
        "x = textscan('1 2', '%5.2f %3d8');",
        "x = textscan('\"', '%q');",
    ] {
        let _ = it.run_text(code);
    }
    run(&mut it, "R = readtable('r.csv');");
    check(&mut it, "isequal(size(R), [2 3])");
    check(
        &mut it,
        "isequal(R.Properties.VariableNames, {'a', 'b', 'ExtraVar1'})",
    );
    run(&mut it, "Z = readtable('z.csv');");
    check(&mut it, "isequal(size(Z), [0 0])");
    run(&mut it, "L = readcell('lat.csv');");
    check(&mut it, "isequal(L, {char([97 233]), 1})");
}
