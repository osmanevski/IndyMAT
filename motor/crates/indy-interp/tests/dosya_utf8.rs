//! Text file I/O encodings: `fopen`'s encoding argument, `char` precision in
//! `fread`/`fwrite`, and decoding in `fgetl`/`fgets`/`fscanf`/`textscan`.

use indy_interp::Interp;

/// An interpreter whose current directory is a fresh temporary folder.
fn interp(name: &str) -> Interp {
    let dir = std::env::temp_dir().join(format!("indy-utf8-{}-{}", std::process::id(), name));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut it = Interp::new();
    run(&mut it, &format!("cd('{}');", dir.display()));
    it
}

fn run(it: &mut Interp, code: &str) {
    if let Err(e) = it.run_text(code) {
        panic!("{} -> error: {}", code, e.message);
    }
}

fn check(it: &mut Interp, cond: &str) {
    let code = format!("t__ = isequal({}, true);", cond);
    if let Err(e) = it.run_text(&code) {
        panic!("{} -> error: {}", cond, e.message);
    }
    let ok = it.get_var("t__").and_then(|v| v.scalar_f64()) == Some(1.0);
    assert!(ok, "failed: {}", cond);
}

/// Creates file `name` holding exactly `bytes`.
fn put(it: &mut Interp, name: &str, bytes: &[u8]) {
    let list: Vec<String> = bytes.iter().map(|b| b.to_string()).collect();
    run(
        it,
        &format!(
            "f__ = fopen('{}', 'wb'); fwrite(f__, [{}], 'uint8'); fclose(f__);",
            name,
            list.join(" ")
        ),
    );
}

fn with_bom(text: &str) -> Vec<u8> {
    let mut b = vec![0xEF, 0xBB, 0xBF];
    b.extend_from_slice(text.as_bytes());
    b
}

#[test]
fn fread_char_decodes_and_keeps_bom() {
    let mut it = interp("fread");
    put(&mut it, "b.txt", &with_bom("İzmir\n"));
    for prec in ["*char", "char=>char", "char*1=>char"] {
        run(
            &mut it,
            &format!("f = fopen('b.txt'); t = fread(f, Inf, '{prec}')'; fclose(f);"),
        );
        check(&mut it, "double(t(1)) == 65279");
        check(&mut it, "strcmp(t(2:6), 'İzmir')");
        check(&mut it, "numel(t) == 7");
    }
    run(
        &mut it,
        "f = fopen('b.txt'); d = fread(f, 2, 'char')'; fclose(f);",
    );
    check(&mut it, "isequal(d, [65279 304])");
    check(&mut it, "strcmp(class(d), 'double')");
}

#[test]
fn fread_bytes_stay_bytes() {
    let mut it = interp("bytes");
    put(&mut it, "b.txt", "İ".as_bytes());
    run(
        &mut it,
        "f = fopen('b.txt'); a = fread(f)'; frewind(f); b = fread(f, Inf, '*uint8')'; \
         frewind(f); c = fread(f, Inf, 'uchar=>char')'; frewind(f); d = fread(f, Inf, '*uchar')'; fclose(f);",
    );
    check(&mut it, "isequal(a, [196 176])");
    check(&mut it, "isequal(b, uint8([196 176])) && isa(b, 'uint8')");
    check(&mut it, "isequal(double(c), [196 176]) && ischar(c)");
    check(&mut it, "isequal(d, uint8([196 176]))");
}

#[test]
fn fread_size_counts_characters_and_ftell_counts_bytes() {
    let mut it = interp("size");
    put(&mut it, "b.txt", "ğüş".as_bytes());
    run(
        &mut it,
        "f = fopen('b.txt'); a = fread(f, 2, '*char')'; p = ftell(f); b = fread(f, 5, '*char')'; \
         q = ftell(f); e = feof(f); st = fseek(f, 2, 'bof'); c = fread(f, 1, '*char'); \
         frewind(f); m = fread(f, [2 2], '*char'); fclose(f);",
    );
    check(&mut it, "strcmp(a, 'ğü')");
    check(&mut it, "p == 4");
    check(&mut it, "strcmp(b, 'ş')");
    check(&mut it, "q == 6");
    check(&mut it, "e == true");
    check(&mut it, "st == 0 && strcmp(c, 'ü')");
    check(
        &mut it,
        "isequal(size(m), [2 2]) && m(1,1) == 'ğ' && m(2,2) == 0",
    );
}

#[test]
fn invalid_utf8_gives_replacement_characters() {
    let mut it = interp("invalid");
    let bad: &[u8] = &[b'A', 0xFF, 0xE2, 0x82];
    put(&mut it, "b.bin", bad);
    run(
        &mut it,
        "f = fopen('b.bin'); a = fread(f, Inf, '*char')'; p = ftell(f); frewind(f); \
         b = fgetl(f); frewind(f); s = fscanf(f, '%s'); frewind(f); c = textscan(f, '%s'); fclose(f);",
    );
    check(&mut it, "isequal(double(a), [65 65533 65533])");
    check(&mut it, "p == 4");
    check(&mut it, "isequal(double(b), [65 65533 65533])");
    check(&mut it, "isequal(double(s), [65 65533 65533])");
    check(&mut it, "isequal(double(c{1}{1}), [65 65533 65533])");
    for n in 0..=4 {
        run(
            &mut it,
            &format!(
                "f = fopen('b.bin'); fseek(f, {n}, 'bof'); x = fread(f, 1, '*char'); fclose(f);"
            ),
        );
    }
}

#[test]
fn fwrite_and_fprintf_write_utf8() {
    let mut it = interp("write");
    let want = "[196 159 195 188 197 159 196 176 195 182 195 167]";
    run(
        &mut it,
        "f = fopen('a.txt', 'w'); n = fwrite(f, 'ğüşİöç', 'char'); fclose(f); \
         f = fopen('a.txt'); a = fread(f)'; fclose(f); \
         f = fopen('b.txt', 'w'); fprintf(f, '%s', 'ğüşİöç'); fclose(f); \
         f = fopen('b.txt'); b = fread(f)'; fclose(f); \
         f = fopen('c.txt', 'w'); fwrite(f, \"ğüşİöç\", 'char'); fclose(f); \
         f = fopen('c.txt'); c = fread(f)'; fclose(f); \
         f = fopen('d.txt', 'w'); fprintf(f, 'ğüşİöç'); fclose(f); \
         f = fopen('d.txt'); d = fread(f)'; fclose(f); \
         f = fopen('e.txt', 'w'); fwrite(f, [char(65279) 'a'], 'char'); fclose(f); \
         f = fopen('e.txt'); e = fread(f)'; fclose(f);",
    );
    check(&mut it, "n == 6");
    check(&mut it, &format!("isequal(a, {want})"));
    check(&mut it, &format!("isequal(b, {want})"));
    check(&mut it, &format!("isequal(c, {want})"));
    check(&mut it, &format!("isequal(d, {want})"));
    check(&mut it, "isequal(e, [239 187 191 97])");
}

#[test]
fn fwrite_byte_precisions_still_saturate() {
    let mut it = interp("saturate");
    run(
        &mut it,
        "f = fopen('a.bin', 'w'); fwrite(f, 'ğ'); fwrite(f, 'ğ', 'uchar'); fwrite(f, 200, 'uint8'); fclose(f); \
         f = fopen('a.bin'); a = fread(f)'; fclose(f);",
    );
    check(&mut it, "isequal(a, [255 255 200])");
}

#[test]
fn fopen_records_encoding_and_machine_format() {
    let mut it = interp("query");
    run(
        &mut it,
        "f = fopen('a.txt', 'w'); [n, p, m, e] = fopen(f); fclose(f); \
         g = fopen('a.txt', 'r', 'ieee-be', 'latin1'); [n2, p2, m2, e2] = fopen(g); fclose(g); \
         h = fopen('a.txt', 'rt', 'n', 'windows-1252'); [~, p3, ~, e3] = fopen(h); fclose(h); \
         k = fopen('a.txt', 'r+', 'n', 'us-ascii'); [~, p4, ~, e4] = fopen(k); fclose(k); \
         [n5, p5, m5, e5] = fopen(99); [n6, p6, m6, e6] = fopen(1); only = fopen(f);",
    );
    check(&mut it, "strcmp(n, 'a.txt') && strcmp(p, 'wb')");
    check(&mut it, "strcmp(m, 'ieee-le') && strcmp(e, 'UTF-8')");
    check(&mut it, "strcmp(n2, 'a.txt') && strcmp(p2, 'rb')");
    check(&mut it, "strcmp(m2, 'ieee-be') && strcmp(e2, 'ISO-8859-1')");
    check(&mut it, "strcmp(p3, 'rt') && strcmp(e3, 'windows-1252')");
    check(&mut it, "strcmp(p4, 'r+b') && strcmp(e4, 'US-ASCII')");
    check(
        &mut it,
        "isempty(n5) && isempty(p5) && isempty(m5) && isempty(e5)",
    );
    check(&mut it, "strcmp(n6, 'stdout') && strcmp(e6, 'UTF-8')");
    check(&mut it, "isempty(only)");
}

#[test]
fn unknown_encoding_is_an_error_and_leaves_the_file_alone() {
    let mut it = interp("unknown");
    put(&mut it, "a.txt", b"keep");
    let Err(e) = it.run_text("f = fopen('a.txt', 'w', 'n', 'no-such-encoding');") else {
        panic!("expected an error");
    };
    assert!(e.message.contains("no-such-encoding"), "{}", e.message);
    assert_eq!(e.identifier, "MATLAB:fopen:UnsupportedEncoding");
    run(
        &mut it,
        "f = fopen('a.txt'); a = fread(f, Inf, '*char')'; fclose(f);",
    );
    check(&mut it, "strcmp(a, 'keep')");
    assert!(
        it.run_text("fopen('a.txt', 'r', 'no-such-format');")
            .is_err()
    );
}

#[test]
fn single_byte_encodings_round_trip() {
    let mut it = interp("latin");
    run(
        &mut it,
        "f = fopen('l.txt', 'w', 'n', 'ISO-8859-1'); fwrite(f, 'éü', 'char'); fprintf(f, 'ç\\n'); fclose(f); \
         f = fopen('l.txt'); b = fread(f)'; fclose(f); \
         f = fopen('l.txt', 'r', 'n', 'ISO-8859-1'); t = fread(f, Inf, '*char')'; frewind(f); g = fgetl(f); fclose(f); \
         f = fopen('s.txt', 'w', 'n', 'ISO-8859-1'); n = fwrite(f, 'ğ', 'char'); fclose(f); \
         f = fopen('s.txt'); s = fread(f)'; fclose(f); \
         f = fopen('a.txt', 'w', 'n', 'US-ASCII'); fprintf(f, 'aé'); fclose(f); \
         f = fopen('a.txt'); a = fread(f)'; frewind(f); fclose(f); \
         f = fopen('a.txt', 'r', 'n', 'US-ASCII'); a2 = fread(f, Inf, '*char')'; fclose(f); \
         f = fopen('w.txt', 'w', 'n', 'windows-1252'); fprintf(f, '€é'); fclose(f); \
         f = fopen('w.txt'); w = fread(f)'; fclose(f); \
         f = fopen('w.txt', 'r', 'n', 'cp1252'); w2 = fread(f, Inf, '*char')'; fclose(f); \
         f = fopen('t.txt', 'w', 'n', 'windows-1254'); fprintf(f, 'ğİşĞŞı'); fclose(f); \
         f = fopen('t.txt'); u = fread(f)'; fclose(f); \
         f = fopen('t.txt', 'r', 'n', 'ISO-8859-9'); u2 = fread(f, Inf, '*char')'; fclose(f);",
    );
    check(&mut it, "isequal(b, [233 252 231 10])");
    check(&mut it, "strcmp(t, sprintf('éüç\\n'))");
    check(&mut it, "strcmp(g, 'éüç')");
    check(&mut it, "n == 1 && isequal(s, 26)");
    check(&mut it, "isequal(a, [97 26])");
    check(&mut it, "isequal(double(a2), [97 26])");
    check(&mut it, "isequal(w, [128 233])");
    check(&mut it, "strcmp(w2, '€é')");
    check(&mut it, "isequal(u, [240 221 254 208 222 253])");
    check(&mut it, "strcmp(u2, 'ğİşĞŞı')");
}

#[test]
fn fgetl_fgets_decode_and_show_the_bom() {
    let mut it = interp("fgetl");
    put(&mut it, "b.txt", &with_bom("İzmir\r\nğüş\nson"));
    run(
        &mut it,
        "f = fopen('b.txt'); a = fgetl(f); p = ftell(f); b = fgets(f); c = fgetl(f); e = feof(f); d = fgetl(f); fclose(f);",
    );
    check(
        &mut it,
        "double(a(1)) == 65279 && strcmp(a(2:end), 'İzmir')",
    );
    check(&mut it, "p == 11");
    check(&mut it, "strcmp(b, sprintf('ğüş\\n'))");
    check(&mut it, "strcmp(c, 'son')");
    check(&mut it, "e == true");
    check(&mut it, "isequal(d, -1)");
    put(&mut it, "l.txt", &[0xFD, 0xF0, b'\n']);
    run(
        &mut it,
        "f = fopen('l.txt', 'r', 'n', 'ISO-8859-9'); a = fgetl(f); fclose(f);",
    );
    check(&mut it, "strcmp(a, 'ığ')");
}

#[test]
fn fscanf_decodes_and_tracks_the_byte_position() {
    let mut it = interp("fscanf");
    put(&mut it, "b.txt", &with_bom("ğüş 12 34\n56"));
    run(
        &mut it,
        "f = fopen('b.txt'); s = fscanf(f, '%s', 1); frewind(f); w = fscanf(f, '%s'); frewind(f); \
         fseek(f, 9, 'bof'); [v, n] = fscanf(f, '%d', 1); p = ftell(f); r = fscanf(f, '%d')'; e = feof(f); fclose(f);",
    );
    check(&mut it, "double(w(1)) == 65279 && strcmp(w(2:4), 'ğüş')");
    check(&mut it, "isequal(v, 12) && n == 1");
    check(&mut it, "p == 12");
    check(&mut it, "isequal(r, [34 56])");
    check(&mut it, "e == true");
    assert!(it.run_text("fscanf(99, '%d');").is_err());
}

#[test]
fn textscan_on_a_fid_decodes_and_keeps_the_bom() {
    let mut it = interp("textscan");
    put(&mut it, "b.txt", &with_bom("İzmir 1\nğüş 2\n"));
    run(
        &mut it,
        "f = fopen('b.txt'); c = textscan(f, '%s %f'); e = feof(f); fclose(f); \
         f = fopen('b.txt'); fseek(f, 3, 'bof'); d = textscan(f, '%s %f', 1); p = ftell(f); g = fgetl(f); h = fgetl(f); fclose(f);",
    );
    check(&mut it, "double(c{1}{1}(1)) == 65279");
    check(
        &mut it,
        "strcmp(c{1}{1}(2:end), 'İzmir') && strcmp(c{1}{2}, 'ğüş')",
    );
    check(&mut it, "isequal(c{2}, [1; 2])");
    check(&mut it, "e == true");
    check(&mut it, "strcmp(d{1}{1}, 'İzmir') && isequal(d{2}, 1)");
    check(&mut it, "p == 11");
    check(&mut it, "isempty(g) && strcmp(h, 'ğüş 2')");
}

#[test]
fn path_based_readers_still_skip_the_bom() {
    let mut it = interp("path");
    put(&mut it, "b.csv", &with_bom("İzmir,1\nAnkara,2\n"));
    run(
        &mut it,
        "C = readcell('b.csv'); L = readlines('b.csv'); \
         T = readtable('b.csv', 'ReadVariableNames', false);",
    );
    check(&mut it, "isequal(C, {'İzmir', 1; 'Ankara', 2})");
    check(&mut it, "strcmp(char(L(1)), 'İzmir,1')");
    check(&mut it, "strcmp(T{1, 1}{1}, 'İzmir')");
}

#[test]
fn read_write_modes_can_read_text() {
    let mut it = interp("plus");
    run(
        &mut it,
        "f = fopen('a.txt', 'w+'); fprintf(f, 'ğx\\nşy\\n'); frewind(f); a = fgetl(f); p = ftell(f); \
         b = fgetl(f); e = feof(f); c = fgetl(f); fclose(f); \
         f = fopen('a.txt', 'r+'); s = fscanf(f, '%c', 2); t = textscan(f, '%s'); fclose(f); \
         f = fopen('a.txt', 'w'); e2 = feof(f); g = fgetl(f); fclose(f);",
    );
    check(&mut it, "strcmp(a, 'ğx') && p == 4");
    check(&mut it, "strcmp(b, 'şy') && e == true && isequal(c, -1)");
    check(&mut it, "strcmp(s, 'ğx') && strcmp(t{1}{1}, 'şy')");
    check(&mut it, "e2 == false && isequal(g, -1)");
    assert!(
        it.run_text("f = fopen('a.txt', 'w'); textscan(f, '%s');")
            .is_err()
    );
}

#[test]
fn machine_format_is_the_default_byte_order() {
    let mut it = interp("endian");
    run(
        &mut it,
        "f = fopen('a.bin', 'w', 'ieee-be'); fwrite(f, 258, 'uint16'); fwrite(f, 258, 'uint16', 0, 'ieee-le'); fclose(f); \
         f = fopen('a.bin'); b = fread(f)'; fclose(f); \
         f = fopen('a.bin', 'r', 'b'); v = fread(f, 1, 'uint16'); w = fread(f, 1, 'uint16', 0, 'l'); fclose(f);",
    );
    check(&mut it, "isequal(b, [1 2 2 1])");
    check(&mut it, "v == 258 && w == 258");
}
