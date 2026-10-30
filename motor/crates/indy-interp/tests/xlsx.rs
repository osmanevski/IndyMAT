//! `.xlsx` workbooks through readtable, writetable, readmatrix, writematrix,
//! readcell and writecell.

use std::path::PathBuf;

use indy_interp::Interp;

fn interp(tag: &str) -> Interp {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("xlsx_{}", tag));
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

#[test]
fn table_round_trip_keeps_types() {
    let mut it = interp("table");
    run(
        &mut it,
        "T = table([1.5; -2; 1e300], {'a'; 'b & <c>'; 'çş'}, [true; false; true], ...
             'VariableNames', {'n', 's', 'b'});
         writetable(T, 't.xlsx'); R = readtable('t.xlsx');",
    );
    check(
        &mut it,
        "isequal(R.Properties.VariableNames, {'n', 's', 'b'})",
    );
    check(&mut it, "isequal(R.n, [1.5; -2; 1e300])");
    check(&mut it, "isequal(R.s, {'a'; 'b & <c>'; 'çş'})");
    check(
        &mut it,
        "islogical(R.b) && isequal(R.b, [true; false; true])",
    );
}

#[test]
fn doubles_keep_every_bit() {
    let mut it = interp("doubles");
    run(
        &mut it,
        "x = [pi; 0.1; 1/3; 1e-300; 123456789012345678; -0.5];
         writematrix(x, 'd.xlsx'); y = readmatrix('d.xlsx');",
    );
    check(&mut it, "isequal(x, y)");
}

#[test]
fn nan_is_blank_and_inf_survives() {
    let mut it = interp("special");
    run(
        &mut it,
        "writematrix([1 NaN; Inf -Inf], 's.xlsx'); y = readmatrix('s.xlsx');
         c = readcell('s.xlsx');",
    );
    check(&mut it, "isequaln(y, [1 NaN; Inf -Inf])");
    check(&mut it, "ismissing(c{1, 2})");
}

#[test]
fn matrix_classes_and_logical_cells() {
    let mut it = interp("classes");
    run(
        &mut it,
        "writematrix(single(0.1), 'a.xlsx'); a = readmatrix('a.xlsx');
         writematrix(int32([1 2; 3 4]), 'b.xlsx'); b = readmatrix('b.xlsx', 'OutputType', 'int32');
         writematrix([true false], 'c.xlsx'); c = readcell('c.xlsx');
         writematrix([\"x\" \"y\"; \"z\" \"w\"], 'e.xlsx'); e = readcell('e.xlsx');",
    );
    check(&mut it, "a == 0.1");
    check(&mut it, "isa(b, 'int32') && isequal(b, int32([1 2; 3 4]))");
    check(&mut it, "isequal(c, {true, false}) && islogical(c{1})");
    check(&mut it, "isequal(e, {'x', 'y'; 'z', 'w'})");
}

#[test]
fn cell_round_trip_keeps_each_type() {
    let mut it = interp("cells");
    run(
        &mut it,
        "C = {1, 'yazı', true; 2.5, '123', missing};
         writecell(C, 'c.xlsx'); R = readcell('c.xlsx');",
    );
    check(&mut it, "R{1, 1} == 1 && R{2, 1} == 2.5");
    check(&mut it, "strcmp(R{1, 2}, 'yazı')");
    // A number stored as text stays text.
    check(&mut it, "strcmp(R{2, 2}, '123')");
    check(&mut it, "islogical(R{1, 3}) && R{1, 3}");
    check(&mut it, "ismissing(R{2, 3})");
}

#[test]
fn awkward_text_survives() {
    let mut it = interp("text");
    run(
        &mut it,
        "s = {'  iki boşluk  '; sprintf('sat\\nır'); '_x0041_'; '<a b=\"c\">&amp;'; sprintf('k\\001x')};
         writecell(s, 't.xlsx'); r = readcell('t.xlsx');",
    );
    check(&mut it, "isequal(r, s)");
}

#[test]
fn sheets_by_name_and_index_keep_each_other() {
    let mut it = interp("sheets");
    run(
        &mut it,
        "writematrix([1 2], 'k.xlsx');
         writematrix([3 4], 'k.xlsx', 'Sheet', 'Ikinci');
         writematrix([5 6], 'k.xlsx', 'Sheet', 4);
         a = readmatrix('k.xlsx'); b = readmatrix('k.xlsx', 'Sheet', 'Ikinci');
         c = readmatrix('k.xlsx', 'Sheet', 2); d = readmatrix('k.xlsx', 'Sheet', 4);
         e = readmatrix('k.xlsx', 'Sheet', 3);",
    );
    check(
        &mut it,
        "isequal(a, [1 2]) && isequal(b, [3 4]) && isequal(c, [3 4])",
    );
    check(&mut it, "isequal(d, [5 6]) && isempty(e)");
    fails_with(
        &mut it,
        "readmatrix('k.xlsx', 'Sheet', 'Yok')",
        "does not exist",
    );
    fails_with(
        &mut it,
        "readmatrix('k.xlsx', 'Sheet', 9)",
        "does not exist",
    );
    fails_with(
        &mut it,
        "readmatrix('k.xlsx', 'Sheet', 1.5)",
        "does not exist",
    );
    fails_with(
        &mut it,
        "writematrix(1, 'k.xlsx', 'Sheet', 'a/b')",
        "not a valid sheet name",
    );
}

#[test]
fn read_ranges_select_cells() {
    let mut it = interp("ranges");
    run(
        &mut it,
        "M = reshape(1:20, 4, 5); writematrix(M, 'r.xlsx');
         a = readmatrix('r.xlsx', 'Range', 'B2:C3');
         b = readmatrix('r.xlsx', 'Range', 'D3');
         c = readmatrix('r.xlsx', 'Range', 'B:C');
         d = readmatrix('r.xlsx', 'Range', '2:3');
         e = readmatrix('r.xlsx', 'Range', [2 2 3 3]);
         f = readmatrix('r.xlsx', 'Range', 'E4:F5');",
    );
    check(&mut it, "isequal(a, M(2:3, 2:3))");
    check(&mut it, "isequal(b, M(3:4, 4:5))");
    check(&mut it, "isequal(c, M(:, 2:3))");
    check(&mut it, "isequal(d, M(2:3, :))");
    check(&mut it, "isequal(e, M(2:3, 2:3))");
    check(&mut it, "isequaln(f, [20 NaN; NaN NaN])");
    for bad in ["'B2:'", "'Bolge'", "'C3:B2'", "'A1:B'", "'XFE1'", "[1 2 3]"] {
        fails_with(
            &mut it,
            &format!("readmatrix('r.xlsx', 'Range', {})", bad),
            "'Range' must look like",
        );
    }
}

#[test]
fn write_ranges_place_and_clip_data() {
    let mut it = interp("write_ranges");
    run(
        &mut it,
        "writematrix([1 2; 3 4], 'w.xlsx', 'Range', 'C3');
         a = readmatrix('w.xlsx', 'Range', 'C3:D4'); c = readcell('w.xlsx');
         writematrix(reshape(1:9, 3, 3), 'x.xlsx', 'Range', 'A1:B2');
         b = readmatrix('x.xlsx');",
    );
    check(&mut it, "isequal(a, [1 2; 3 4]) && isequal(size(c), [2 2])");
    check(&mut it, "isequal(b, [1 4; 2 5])");
}

#[test]
fn write_modes() {
    let mut it = interp("modes");
    run(
        &mut it,
        "writematrix([1 2; 3 4], 'm.xlsx');
         writematrix(9, 'm.xlsx');
         a = readmatrix('m.xlsx');
         writematrix([7 8], 'm.xlsx', 'WriteMode', 'append');
         b = readmatrix('m.xlsx');
         writematrix(5, 'm.xlsx', 'Sheet', 'Diger');
         writematrix(6, 'm.xlsx', 'WriteMode', 'overwritesheet');
         c = readmatrix('m.xlsx'); d = readmatrix('m.xlsx', 'Sheet', 'Diger');
         writematrix(1, 'm.xlsx', 'WriteMode', 'replacefile');
         T = table([1; 2], 'VariableNames', {'v'});
         writetable(T, 'a.xlsx'); writetable(T, 'a.xlsx', 'WriteMode', 'append');
         R = readtable('a.xlsx');",
    );
    check(&mut it, "isequal(a, [9 2; 3 4])");
    check(&mut it, "isequal(b, [9 2; 3 4; 7 8])");
    check(&mut it, "c == 6 && d == 5");
    fails_with(
        &mut it,
        "readmatrix('m.xlsx', 'Sheet', 'Diger')",
        "does not exist",
    );
    // Appending leaves the header out.
    check(&mut it, "isequal(R.v, [1; 2; 1; 2])");
    fails_with(
        &mut it,
        "writematrix(1, 'm.xlsx', 'WriteMode', 'overwrite')",
        "WriteMode",
    );
    fails_with(
        &mut it,
        "writematrix(1, 'm.xlsx', 'WriteMode', 'append', 'Range', 'A1')",
        "cannot be combined",
    );
}

#[test]
fn table_options() {
    let mut it = interp("table_options");
    run(
        &mut it,
        "T = table([1; 2], [3; 4], 'VariableNames', {'a', 'b'}, 'RowNames', {'x'; 'y'});
         writetable(T, 'o.xlsx', 'WriteRowNames', true);
         R = readtable('o.xlsx', 'ReadRowNames', true);
         writetable(T, 'n.xlsx', 'WriteVariableNames', false);
         N = readtable('n.xlsx');
         writetable(T, 's.xlsx', 'Sheet', 'Veri', 'Range', 'B2');
         S = readtable('s.xlsx', 'Sheet', 'Veri', 'Range', 'B2');
         M = table([1 2; 3 4], 'VariableNames', {'m'}); writetable(M, 'm.xlsx');
         Q = readtable('m.xlsx');
         V = readtable('o.xlsx', 'ReadVariableNames', false, 'Range', 'B2:C3');
         [p, q] = readvars('s.xlsx', 'Sheet', 'Veri');",
    );
    check(
        &mut it,
        "isequal(R.a, [1; 2]) && isequal(R.Properties.RowNames, {'x'; 'y'})",
    );
    check(
        &mut it,
        "isequal(N.Var1, [1; 2]) && isequal(N.Var2, [3; 4])",
    );
    check(&mut it, "isequal(S.a, [1; 2]) && isequal(S.b, [3; 4])");
    check(
        &mut it,
        "isequal(Q.Properties.VariableNames, {'m_1', 'm_2'})",
    );
    check(&mut it, "isequal(Q.m_2, [2; 4])");
    check(
        &mut it,
        "isequal(V.Var1, [1; 2]) && isequal(V.Var2, [3; 4])",
    );
    check(&mut it, "isequal(p, [1; 2]) && isequal(q, [3; 4])");
}

#[test]
fn text_only_options_and_other_formats_give_explicit_errors() {
    let mut it = interp("errors");
    run(&mut it, "writematrix(1, 'e.xlsx');");
    let opt = "does not apply to spreadsheet files";
    fails_with(&mut it, "readtable('e.xlsx', 'Delimiter', ',')", opt);
    fails_with(&mut it, "readmatrix('e.xlsx', 'Delimiter', ',')", opt);
    fails_with(&mut it, "readcell('e.xlsx', 'Encoding', 'UTF-8')", opt);
    fails_with(&mut it, "writematrix(1, 'e.xlsx', 'Delimiter', ',')", opt);
    fails_with(
        &mut it,
        "writematrix(1, 'e.xlsx', 'UseExcel', true)",
        "not available",
    );
    run(&mut it, "writematrix(2, 'e.xlsx', 'UseExcel', false);");
    fails_with(&mut it, "writematrix(1+2i, 'e.xlsx')", "complex");
    fails_with(&mut it, "writecell({[1 2]}, 'e.xlsx')", "scalar");
    fails_with(&mut it, "readmatrix('yok.xlsx')", "Unable to find");
    for ext in ["xls", "ods", "xlsm", "xlsb"] {
        let unsupported = "are not supported yet";
        fails_with(&mut it, &format!("readtable('a.{}')", ext), unsupported);
        fails_with(
            &mut it,
            &format!("writematrix(1, 'a.{}')", ext),
            unsupported,
        );
    }
    // Sheet and Range are rejected for text files as before.
    run(&mut it, "writematrix(1, 'e.csv');");
    fails_with(
        &mut it,
        "readmatrix('e.csv', 'Sheet', 1)",
        "not supported yet",
    );
    fails_with(
        &mut it,
        "readtable('e.csv', 'Range', 'A1')",
        "not supported yet",
    );
}

#[test]
fn dates_are_real_date_cells() {
    let mut it = interp("dates");
    run(
        &mut it,
        "d = datetime(2024, 3, 15, 12, 30, 0) + days(0:1)';
         T = table(d, [1; 2], 'VariableNames', {'t', 'n'});
         writetable(T, 'd.xlsx'); R = readtable('d.xlsx'); C = readcell('d.xlsx');",
    );
    check(
        &mut it,
        "isa(R.t, 'datetime') && all(abs(seconds(R.t - d)) < 1e-3)",
    );
    check(&mut it, "isequal(R.n, [1; 2])");
    check(
        &mut it,
        "isa(C{2, 1}, 'datetime') && abs(seconds(C{2, 1} - d(1))) < 1e-3",
    );
}

/// A workbook as another producer lays it out: inline strings, shared strings
/// with rich runs and phonetics, prefixed names, no `r` attributes, absolute
/// targets, a custom date format, the 1904 system and a stored zip.
fn foreign_workbook() -> Vec<u8> {
    let parts: Vec<(&str, String)> = vec![
        (
            "_rels/.rels",
            r#"<Relationships xmlns="x"><Relationship Id="a" Type="t/officeDocument" Target="/book/main.xml"/></Relationships>"#.to_string(),
        ),
        (
            "book/main.xml",
            r#"<?xml version="1.0"?><!-- c --><x:workbook xmlns:x="m" xmlns:r="r"><x:workbookPr date1904="1"/><x:sheets>
               <x:sheet name="Bir" sheetId="7" r:id="s1"/><x:sheet name="&#304;ki" sheetId="9" r:id="s2"/></x:sheets></x:workbook>"#.to_string(),
        ),
        (
            "book/_rels/main.xml.rels",
            r#"<Relationships xmlns="x">
               <Relationship Id="s1" Type="t/worksheet" Target="/data/one.xml"/>
               <Relationship Id="s2" Type="t/worksheet" Target="../data/two.xml"/>
               <Relationship Id="st" Type="t/styles" Target="st.xml"/>
               <Relationship Id="ss" Type="t/sharedStrings" Target="ss.xml"/></Relationships>"#.to_string(),
        ),
        (
            "book/st.xml",
            r#"<styleSheet><numFmts count="1"><numFmt numFmtId="170" formatCode="yyyy\-mm\-dd"/></numFmts>
               <cellStyleXfs count="1"><xf numFmtId="14"/></cellStyleXfs>
               <cellXfs count="2"><xf numFmtId="0"/><xf numFmtId="170"/></cellXfs></styleSheet>"#.to_string(),
        ),
        (
            "book/ss.xml",
            r#"<sst><si><r><t>zen</t></r><r><t xml:space="preserve"> gin</t></r><rPh><t>skip</t></rPh></si><si/><si><t><![CDATA[<raw>&]]></t></si></sst>"#.to_string(),
        ),
        (
            "data/one.xml",
            r#"<worksheet><sheetData><row><c t="s"><v>0</v></c><c t="inlineStr"><is><t>sat&#305;r</t></is></c><c/><c t="b"><v>1</v></c></row>
               <row r="3"><c r="B3"><f>1+1</f><v>2</v></c><c r="C3" t="str"><f>"a"</f><v>a</v></c><c r="D3" t="e"><v>#N/A</v></c></row>
               <row r="4"><c r="A4" s="1"><v>0</v></c><c r="B4" t="s"><v>1</v></c><c r="C4" t="s"><v>2</v></c></row></sheetData></worksheet>"#.to_string(),
        ),
        (
            "data/two.xml",
            r#"<worksheet><sheetData><row r="2"><c r="B2"><v>5</v></c></row></sheetData></worksheet>"#.to_string(),
        ),
    ];
    let mut out = Vec::new();
    let mut dir = Vec::new();
    for (name, text) in &parts {
        let data = text.as_bytes();
        let crc = indy_value::deflate::crc32(data);
        let offset = out.len() as u32;
        let mut common = Vec::new();
        common.extend_from_slice(&[20, 0, 0, 0, 0, 0, 0, 0, 0x21, 0]);
        common.extend_from_slice(&crc.to_le_bytes());
        common.extend_from_slice(&(data.len() as u32).to_le_bytes());
        common.extend_from_slice(&(data.len() as u32).to_le_bytes());
        common.extend_from_slice(&(name.len() as u16).to_le_bytes());
        common.extend_from_slice(&[0, 0]);
        out.extend_from_slice(b"PK\x03\x04");
        out.extend_from_slice(&common);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);
        dir.extend_from_slice(b"PK\x01\x02\x14\x00");
        dir.extend_from_slice(&common);
        dir.extend_from_slice(&[0; 10]);
        dir.extend_from_slice(&offset.to_le_bytes());
        dir.extend_from_slice(name.as_bytes());
    }
    let start = out.len() as u32;
    out.extend_from_slice(&dir);
    out.extend_from_slice(b"PK\x05\x06\0\0\0\0");
    out.extend_from_slice(&(parts.len() as u16).to_le_bytes());
    out.extend_from_slice(&(parts.len() as u16).to_le_bytes());
    out.extend_from_slice(&(dir.len() as u32).to_le_bytes());
    out.extend_from_slice(&start.to_le_bytes());
    out.extend_from_slice(&[0, 0]);
    out
}

#[test]
fn reads_workbooks_from_other_producers() {
    let mut it = interp("foreign");
    std::fs::write(it.cwd.join("f.xlsx"), foreign_workbook()).unwrap();
    run(
        &mut it,
        "c = readcell('f.xlsx'); i = readmatrix('f.xlsx', 'Sheet', 'İki');",
    );
    check(&mut it, "isequal(size(c), [3 4])");
    check(
        &mut it,
        "strcmp(c{1, 1}, 'zen gin') && strcmp(c{1, 2}, 'satır')",
    );
    check(&mut it, "ismissing(c{1, 3}) && islogical(c{1, 4})");
    check(
        &mut it,
        "c{2, 2} == 2 && strcmp(c{2, 3}, 'a') && strcmp(c{2, 4}, '#N/A')",
    );
    // Serial 0 of the 1904 system is 1 January 1904.
    check(&mut it, "isa(c{3, 1}, 'datetime') && year(c{3, 1}) == 1904");
    check(&mut it, "ismissing(c{3, 2}) && strcmp(c{3, 3}, '<raw>&')");
    check(&mut it, "i == 5");
}

#[test]
fn corrupt_files_are_errors() {
    let mut it = interp("corrupt");
    std::fs::write(it.cwd.join("c.xlsx"), b"this is not a zip").unwrap();
    fails_with(&mut it, "readtable('c.xlsx')", "not a valid .xlsx");
    fails_with(&mut it, "readmatrix('c.xlsx')", "not a valid .xlsx");
    // Never overwrite a file that cannot be read back, unless asked to.
    fails_with(&mut it, "writematrix(1, 'c.xlsx')", "replacefile");
    assert_eq!(
        std::fs::read(it.cwd.join("c.xlsx")).unwrap(),
        b"this is not a zip"
    );
    run(
        &mut it,
        "writematrix(1, 'c.xlsx', 'WriteMode', 'replacefile');",
    );
    check(&mut it, "readmatrix('c.xlsx') == 1");
}
