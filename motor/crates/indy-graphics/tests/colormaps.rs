//! Colormaps against values measured from Octave 11.3, column-major:
//! `tests/oracle/colormaps.json` (11 maps × 14 sizes) and
//! `tests/oracle/colormaps2.json` (11 more maps × 14 sizes, minus `ocean(2)`,
//! which errors in Octave).

use indy_graphics::color::{COLORMAP_NAMES, colormap_by_name};
use indy_graphics::json::{self, Json};

#[test]
fn colormaps_match_octave() {
    let checked = check_oracle(include_str!("oracle/colormaps.json"));
    assert_eq!(checked, 11 * 14);
}

#[test]
fn colormaps2_match_octave() {
    let checked = check_oracle(include_str!("oracle/colormaps2.json"));
    assert_eq!(checked, 11 * 14 - 1);
}

#[test]
fn every_name_builds_any_size() {
    for name in COLORMAP_NAMES {
        for n in [0, 1, 2, 3, 10, 27, 64, 300] {
            let map = colormap_by_name(name, n).unwrap_or_else(|| panic!("{name} missing"));
            assert_eq!(map.len(), n, "{name}({n})");
            assert!(
                map.iter().flatten().all(|v| (0.0..=1.0).contains(v)),
                "{name}({n}) out of range"
            );
        }
    }
}

/// Compares every measured map in `text` to ours (tolerance 1e-12) and
/// returns how many maps were checked.
fn check_oracle(text: &str) -> usize {
    let root = json::parse(text.trim()).expect("colormap oracle");
    let Json::Obj(maps) = &root else {
        panic!("object expected")
    };
    let mut checked = 0;
    let mut worst = 0.0f64;
    let mut bad = Vec::new();
    for (name, sizes) in maps {
        let Json::Obj(sizes) = sizes else { continue };
        let mut map_worst = 0.0f64;
        for (_, entry) in sizes {
            let rows = entry.get("rows").and_then(Json::as_f64).unwrap() as usize;
            let want = entry.get("values").and_then(Json::as_f64_vec).unwrap();
            let got = colormap_by_name(name, rows).unwrap_or_else(|| panic!("{name} missing"));
            assert_eq!(got.len(), rows, "{name}({rows}) rows");
            for (i, row) in got.iter().enumerate() {
                for c in 0..3 {
                    let d = (row[c] - want[i + c * rows]).abs();
                    map_worst = map_worst.max(d);
                    if d >= 1e-12
                        && !bad
                            .iter()
                            .any(|b: &String| b.starts_with(&format!("{name}({rows})")))
                    {
                        bad.push(format!(
                            "{name}({rows}) row {i} ch {c}: {} vs {}",
                            row[c],
                            want[i + c * rows]
                        ));
                    }
                }
            }
            checked += 1;
        }
        eprintln!("  {name}: worst difference {map_worst:e}");
        worst = worst.max(map_worst);
    }
    assert!(
        bad.is_empty(),
        "{} mismatching maps:\n{}",
        bad.len(),
        bad.join("\n")
    );
    eprintln!("colormaps: {checked} measured maps, worst difference {worst:e}");
    checked
}
