//! WAV files (audiowrite, audioread, audioinfo) through the interpreter.

use std::path::PathBuf;

use indy_interp::Interp;

fn interp(tag: &str) -> Interp {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("ses_{}", tag));
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
fn default_is_16_bit_with_the_expected_precision() {
    let mut it = interp("default");
    run(
        &mut it,
        "y = sin(2*pi*440*(0:799)'/8000)*0.5; audiowrite('a.wav', y, 8000);
         [z, fs] = audioread('a.wav'); i = audioinfo('a.wav');",
    );
    check(&mut it, "fs == 8000 && isequal(size(z), [800 1])");
    check(&mut it, "max(abs(z - y)) <= 1/32768");
    check(&mut it, "i.BitsPerSample == 16 && i.NumChannels == 1");
    check(&mut it, "i.TotalSamples == 800 && i.Duration == 0.1");
    check(&mut it, "strcmp(i.CompressionMethod, 'Uncompressed')");
}

#[test]
fn every_bit_depth_round_trips_with_the_documented_native_class() {
    let mut it = interp("depths");
    run(&mut it, "y = [-0.5; -0.25; 0; 0.25; 0.5]; w = 1;");
    for (bits, class, tol) in [
        (8, "uint8", "1/128"),
        (16, "int16", "1/32768"),
        (24, "int32", "2^-23"),
        (32, "single", "1e-7"),
        (64, "double", "0"),
    ] {
        run(
            &mut it,
            &format!(
                "audiowrite('d.wav', y, 1000, 'BitsPerSample', {b});
                 z = audioread('d.wav'); n = audioread('d.wav', 'native');
                 i = audioinfo('d.wav');",
                b = bits
            ),
        );
        check(&mut it, &format!("max(abs(z - y)) <= {}", tol));
        check(&mut it, &format!("strcmp(class(n), '{}')", class));
        check(&mut it, &format!("i.BitsPerSample == {}", bits));
    }
}

#[test]
fn integer_inputs_are_stored_unchanged_at_their_own_depth() {
    let mut it = interp("ints");
    run(
        &mut it,
        "u = uint8([0; 1; 128; 255]); audiowrite('u.wav', u, 100, 'BitsPerSample', 8);
         a = audioread('u.wav', 'native');
         s = int16([-32768; -1; 0; 32767]); audiowrite('s.wav', s, 100);
         b = audioread('s.wav', 'native');
         l = int32([-2147483648; -1; 0; 2147483647]);
         audiowrite('l.wav', l, 100, 'BitsPerSample', 32);
         c = audioread('l.wav', 'native');",
    );
    check(&mut it, "isequal(a, u) && isa(a, 'uint8')");
    check(&mut it, "isequal(b, s) && isa(b, 'int16')");
    check(&mut it, "isequal(c, l) && isa(c, 'int32')");
}

#[test]
fn an_integer_input_converts_to_another_depth_by_full_scale() {
    let mut it = interp("convert");
    run(
        &mut it,
        "audiowrite('c.wav', int16([-32768; 16384]), 100, 'BitsPerSample', 32);
         n = audioread('c.wav', 'native');
         audiowrite('e.wav', uint8([0; 192]), 100);
         m = audioread('e.wav');",
    );
    check(&mut it, "isa(n, 'single') && isequal(double(n), [-1; 0.5])");
    check(&mut it, "isequal(m, [-1; 0.5])");
}

#[test]
fn twenty_four_bit_native_is_at_the_top_of_int32() {
    let mut it = interp("native24");
    run(
        &mut it,
        "audiowrite('n.wav', [-1; 0.5], 100, 'BitsPerSample', 24);
         n = audioread('n.wav', 'native');",
    );
    check(&mut it, "isequal(n, int32([-2147483648; 1073741824]))");
}

#[test]
fn channels_are_columns() {
    let mut it = interp("channels");
    run(
        &mut it,
        "y = [0.1 -0.1 0.2; 0.3 -0.3 0.4]; audiowrite('m.wav', y, 48000, 'BitsPerSample', 64);
         [z, fs] = audioread('m.wav'); i = audioinfo('m.wav');
         audiowrite('r.wav', [0.25 0.5 0.75], 8000, 'BitsPerSample', 64);
         r = audioread('r.wav');",
    );
    check(
        &mut it,
        "isequal(z, y) && fs == 48000 && i.NumChannels == 3",
    );
    check(&mut it, "isequal(r, [0.25; 0.5; 0.75])");
}

#[test]
fn clipping_saturates_and_warns() {
    let mut it = interp("clip");
    run(
        &mut it,
        "lastwarn(''); audiowrite('k.wav', [-2; 2; 0], 100);
         [m, id] = lastwarn(); z = audioread('k.wav', 'native');
         lastwarn(''); audiowrite('f.wav', [-2; 2], 100, 'BitsPerSample', 32);
         [m2, ~] = lastwarn(); f = audioread('f.wav');
         lastwarn(''); audiowrite('o.wav', [-1; 1], 100); [m3, ~] = lastwarn();",
    );
    check(&mut it, "isequal(z, int16([-32768; 32767; 0]))");
    check(&mut it, "strcmp(id, 'MATLAB:audiowrite:dataClipped')");
    check(
        &mut it,
        "isequal(f, [-1; 1]) && ~isempty(m2) && ~isempty(m)",
    );
    check(&mut it, "isempty(m3)");
}

#[test]
fn sample_ranges_select_frames() {
    let mut it = interp("range");
    run(
        &mut it,
        "y = (1:6)'/8; audiowrite('s.wav', y, 100, 'BitsPerSample', 64);
         a = audioread('s.wav', [2 4]); b = audioread('s.wav', [5 Inf]);
         c = audioread('s.wav', [1 2], 'native');",
    );
    check(&mut it, "isequal(a, y(2:4)) && isequal(b, y(5:6))");
    check(&mut it, "isequal(c, y(1:2))");
    fails_with(&mut it, "audioread('s.wav', [0 2])", "sample range");
    fails_with(&mut it, "audioread('s.wav', [3 2])", "sample range");
    fails_with(&mut it, "audioread('s.wav', [1 7])", "sample range");
    fails_with(&mut it, "audioread('s.wav', 2)", "sample range");
    fails_with(
        &mut it,
        "audioread('s.wav', 'single')",
        "'double' or 'native'",
    );
}

#[test]
fn metadata_round_trips() {
    let mut it = interp("meta");
    run(
        &mut it,
        "audiowrite('t.wav', [0; 0.5], 100, 'Title', 'Baslik', 'Artist', 'Ben', 'Comment', 'Not');
         i = audioinfo('t.wav'); z = audioread('t.wav');
         audiowrite('n.wav', [0; 0.5], 100); j = audioinfo('n.wav');",
    );
    check(
        &mut it,
        "strcmp(i.Title, 'Baslik') && strcmp(i.Artist, 'Ben')",
    );
    check(&mut it, "strcmp(i.Comment, 'Not') && isequal(z, [0; 0.5])");
    check(
        &mut it,
        "isempty(j.Title) && isempty(j.Artist) && isempty(j.Comment)",
    );
}

#[test]
fn a_missing_extension_means_wav() {
    let mut it = interp("noext");
    run(
        &mut it,
        "audiowrite('x', [0; 0.5], 100); z = audioread('x.wav');",
    );
    check(&mut it, "isequal(z, [0; 0.5])");
}

#[test]
fn unsupported_things_give_explicit_errors() {
    let mut it = interp("errors");
    for (ext, label) in [
        ("flac", "FLAC"),
        ("mp3", "MP3"),
        ("ogg", "OGG"),
        ("m4a", "MPEG-4 audio"),
    ] {
        let not_yet = format!("{} files are not supported yet in the Rust engine", label);
        fails_with(
            &mut it,
            &format!("audiowrite('a.{}', 0, 100)", ext),
            &not_yet,
        );
        fails_with(&mut it, &format!("audioread('a.{}')", ext), &not_yet);
        fails_with(&mut it, &format!("audioinfo('a.{}')", ext), &not_yet);
    }
    for f in ["sound", "soundsc", "audioplayer", "audiorecorder"] {
        let msg = format!("{}: no audio output in the Rust engine", f);
        fails_with(&mut it, &format!("{}(0, 100)", f), &msg);
    }
    fails_with(
        &mut it,
        "audiowrite('a.wav', int8(1), 100)",
        "not supported",
    );
    fails_with(&mut it, "audiowrite('a.wav', 1+2i, 100)", "must be real");
    fails_with(&mut it, "audiowrite('a.wav', [], 100)", "must not be empty");
    fails_with(&mut it, "audiowrite('a.wav', 0, -1)", "sample rate");
    fails_with(
        &mut it,
        "audiowrite('a.wav', 0, 100, 'BitsPerSample', 12)",
        "BitsPerSample",
    );
    fails_with(
        &mut it,
        "audiowrite('a.wav', 0, 100, 'BitRate', 128)",
        "not supported",
    );
    fails_with(&mut it, "audioread('yok.wav')", "does not exist");
}

#[test]
fn corrupt_files_are_errors_and_never_panic() {
    let mut it = interp("corrupt");
    run(
        &mut it,
        "audiowrite('g.wav', [0.1 0.2; 0.3 0.4], 100, 'Title', 'T');",
    );
    let good = std::fs::read(it.cwd.join("g.wav")).unwrap();
    // Every truncation and every single-byte change: an answer or an error.
    for n in 0..good.len() {
        std::fs::write(it.cwd.join("c.wav"), &good[..n]).unwrap();
        let _ = it.run_text("c = audioread('c.wav'); d = audioinfo('c.wav');");
        for v in [0x00, 0xFF, 0x7F] {
            let mut bad = good.clone();
            bad[n] = v;
            std::fs::write(it.cwd.join("c.wav"), &bad).unwrap();
            let _ = it.run_text("c = audioread('c.wav'); d = audioinfo('c.wav');");
        }
    }
    std::fs::write(it.cwd.join("c.wav"), b"not a wave file").unwrap();
    fails_with(&mut it, "audioread('c.wav')", "not a WAVE");
    // A data chunk claiming 4 GB in a tiny file must not allocate that much.
    let mut huge = good.clone();
    let at = huge.windows(4).position(|w| w == b"data").unwrap();
    huge[at + 4..at + 8].copy_from_slice(&u32::MAX.to_le_bytes());
    std::fs::write(it.cwd.join("c.wav"), &huge).unwrap();
    run(&mut it, "c = audioread('c.wav');");
}
