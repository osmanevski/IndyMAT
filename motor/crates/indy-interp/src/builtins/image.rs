//! Image files are not implemented yet: these names say so explicitly.

use super::{err, reg};
use crate::{Interp, R};
use indy_value::Value;

fn unsupported(name: &str) -> R<Vec<Value>> {
    Err(err(
        "IndyR:imageUnsupported",
        format!(
            "{}: image files are not supported yet in the Rust engine",
            name
        ),
    ))
}

pub fn register(it: &mut Interp) {
    reg(it, "imread", |_, _, _| unsupported("imread"));
    reg(it, "imwrite", |_, _, _| unsupported("imwrite"));
    reg(it, "imfinfo", |_, _, _| unsupported("imfinfo"));
}
