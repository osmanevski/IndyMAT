//! Concatenation (`[a, b; c]`, `cat`, `horzcat`, `vertcat`) with MATLAB's
//! class rules: cell > struct > char > integer > single > double > logical.

use crate::{Array, Error, Num, NumClass, Res, StructArray, Value, norm_dims, numel_of};

fn mismatch() -> Box<Error> {
    Error::new(
        "MATLAB:catenate:dimensionMismatch",
        "Dimensions of arrays being concatenated are not consistent.",
    )
}

fn dim_at(d: &[usize], k: usize) -> usize {
    *d.get(k).unwrap_or(&1)
}

/// Output dims for concatenation along `d` (0-based) or an error.
fn out_dims(parts: &[Vec<usize>], d: usize) -> Res<Vec<usize>> {
    let nd = parts.iter().map(|p| p.len()).max().unwrap_or(2).max(d + 1);
    let mut out: Vec<usize> = (0..nd).map(|k| dim_at(&parts[0], k)).collect();
    out[d] = 0;
    for p in parts {
        for k in 0..nd {
            if k == d {
                out[d] += dim_at(p, k);
            } else if dim_at(p, k) != out[k] {
                return Err(mismatch());
            }
        }
    }
    Ok(norm_dims(out))
}

fn cat_data<T: Clone>(parts: &[(&[usize], &[T])], d: usize) -> Vec<T> {
    let total: usize = parts.iter().map(|p| p.1.len()).sum();
    let mut out = Vec::with_capacity(total);
    if parts.is_empty() {
        return out;
    }
    let outer: usize = parts[0].0.iter().skip(d + 1).product();
    let slabs: Vec<usize> = parts
        .iter()
        .map(|(dims, _)| (0..=d).map(|k| dim_at(dims, k)).product())
        .collect();
    for o in 0..outer {
        for (i, (_, data)) in parts.iter().enumerate() {
            let s = slabs[i];
            out.extend_from_slice(&data[o * s..(o + 1) * s]);
        }
    }
    out
}

/// Concatenate along dimension `d` (0-based).
pub fn cat(values: Vec<Value>, d: usize) -> Res<Value> {
    if values.len() == 1 {
        return Ok(values.into_iter().next().unwrap());
    }
    // A sparse value going into a cell stays what it is: only the cell code
    // below handles that.
    if values.iter().any(Value::is_sparse) && !values.iter().any(|v| matches!(v, Value::Cell(_))) {
        return crate::sparse::cat(values, d);
    }
    let any_cell = values.iter().any(|v| matches!(v, Value::Cell(_)));
    // Drop [] (0x0) operands, MATLAB style; other empties take part only if
    // they fit.
    let mut vals: Vec<Value> = values
        .into_iter()
        .filter(|v| !(v.is_empty() && v.dims_ref() == [0, 0]))
        .collect();
    if vals.is_empty() {
        return Ok(if any_cell {
            Value::empty_cell()
        } else {
            Value::empty()
        });
    }
    if vals.len() > 1 {
        let dims: Vec<Vec<usize>> = vals.iter().map(|v| v.dims()).collect();
        if out_dims(&dims, d).is_err() {
            // Retry without any empties.
            vals.retain(|v| !v.is_empty());
            if vals.is_empty() {
                return Ok(Value::empty());
            }
        }
    }
    if vals.len() == 1 && !any_cell {
        return Ok(vals.pop().unwrap());
    }
    if any_cell {
        let cells: Vec<Array<Value>> = vals
            .into_iter()
            .map(|v| match v {
                Value::Cell(c) => c,
                other => Array::scalar(other),
            })
            .collect();
        let dims: Vec<Vec<usize>> = cells.iter().map(|c| c.dims.clone()).collect();
        let od = out_dims(&dims, d)?;
        let parts: Vec<(&[usize], &[Value])> = cells
            .iter()
            .map(|c| (c.dims.as_slice(), c.data.as_slice()))
            .collect();
        return Ok(Value::Cell(Array::new(od, cat_data(&parts, d))));
    }
    if vals.iter().any(|v| matches!(v, Value::Str(_))) {
        let arrs: Vec<Array<Option<String>>> = vals.iter().map(crate::to_string_array).collect();
        let dims: Vec<Vec<usize>> = arrs.iter().map(|a| a.dims.clone()).collect();
        let od = out_dims(&dims, d)?;
        let parts: Vec<(&[usize], &[Option<String>])> = arrs
            .iter()
            .map(|a| (a.dims.as_slice(), a.data.as_slice()))
            .collect();
        return Ok(Value::Str(Array::new(od, cat_data(&parts, d))));
    }
    if vals.iter().any(|v| matches!(v, Value::Struct(_))) {
        let mut structs = Vec::new();
        for v in vals {
            match v {
                Value::Struct(s) => structs.push(s),
                _ => {
                    return Err(Error::new(
                        "MATLAB:concatenation:structsAndNonStructs",
                        "Concatenation of a struct with a non-struct value is not allowed.",
                    ));
                }
            }
        }
        let fields = structs[0].fields.clone();
        let mut aligned = Vec::new();
        for s in &structs {
            if s.fields.len() != fields.len() || !fields.iter().all(|f| s.field_index(f).is_some())
            {
                return Err(Error::new(
                    "MATLAB:catenate:structFieldBad",
                    "Concatenation of structures requires same field names.",
                ));
            }
            let elems: Vec<Vec<Value>> = s
                .elems
                .iter()
                .map(|e| {
                    fields
                        .iter()
                        .map(|f| e[s.field_index(f).unwrap()].clone())
                        .collect()
                })
                .collect();
            aligned.push((s.dims.clone(), elems));
        }
        let dims: Vec<Vec<usize>> = aligned.iter().map(|a| a.0.clone()).collect();
        let od = out_dims(&dims, d)?;
        let parts: Vec<(&[usize], &[Vec<Value>])> = aligned
            .iter()
            .map(|a| (a.0.as_slice(), a.1.as_slice()))
            .collect();
        return Ok(Value::Struct(StructArray {
            dims: od,
            fields,
            elems: cat_data(&parts, d),
        }));
    }
    if vals.iter().any(Value::is_object_like) {
        let mut class: Option<crate::ObjArray> = None;
        let mut arrs = Vec::new();
        for v in &vals {
            let Some(o) = crate::ObjArray::parts(v) else {
                return Err(Error::new(
                    "MATLAB:concatenation:invalidClass",
                    format!(
                        "Concatenation of {} with {} is not allowed.",
                        vals.iter()
                            .find(|v| v.is_object_like())
                            .unwrap()
                            .class_name(),
                        v.class_name()
                    ),
                ));
            };
            if let Some(k) = &class
                && k.class != o.class
            {
                return Err(Error::new(
                    "MATLAB:concatenation:invalidClass",
                    format!(
                        "Concatenation of objects of classes {} and {} is not allowed.",
                        k.class, o.class
                    ),
                ));
            }
            arrs.push(o.arr.clone());
            class.get_or_insert(o);
        }
        let dims: Vec<Vec<usize>> = arrs.iter().map(|a| a.dims.clone()).collect();
        let od = out_dims(&dims, d)?;
        let parts: Vec<(&[usize], &[Value])> = arrs
            .iter()
            .map(|a| (a.dims.as_slice(), a.data.as_slice()))
            .collect();
        let data = cat_data(&parts, d);
        return Ok(class.unwrap().with(od, data));
    }
    if vals.iter().any(|v| matches!(v, Value::Func(_))) {
        return Err(Error::new(
            "MATLAB:concatenation:nonScalar",
            format!(
                "Nonscalar arrays of {} are not allowed; use a cell array.",
                vals.iter()
                    .find(|v| matches!(v, Value::Func(_)))
                    .unwrap()
                    .class_name()
            ),
        ));
    }
    let dims: Vec<Vec<usize>> = vals.iter().map(|v| v.dims()).collect();
    let od = out_dims(&dims, d)?;
    if vals.iter().all(|v| matches!(v, Value::Bool(_))) {
        let arrs: Vec<&Array<bool>> = vals
            .iter()
            .map(|v| match v {
                Value::Bool(a) => a,
                _ => unreachable!(),
            })
            .collect();
        let parts: Vec<(&[usize], &[bool])> = arrs
            .iter()
            .map(|a| (a.dims.as_slice(), a.data.as_slice()))
            .collect();
        return Ok(Value::Bool(Array::new(od, cat_data(&parts, d))));
    }
    if vals.iter().any(|v| matches!(v, Value::Char(_))) {
        let mut chars: Vec<Array<char>> = Vec::new();
        for v in &vals {
            chars.push(match v {
                Value::Char(a) => a.clone(),
                other => {
                    let n = other.to_num().unwrap();
                    Array::new(
                        n.dims.clone(),
                        n.re.iter()
                            .map(|x| char::from_u32(*x as u32).unwrap_or('\0'))
                            .collect(),
                    )
                }
            });
        }
        let parts: Vec<(&[usize], &[char])> = chars
            .iter()
            .map(|a| (a.dims.as_slice(), a.data.as_slice()))
            .collect();
        return Ok(Value::Char(Array::new(od, cat_data(&parts, d))));
    }
    let nums: Vec<Num> = vals.iter().map(|v| v.to_num().unwrap()).collect();
    let mut class = NumClass::Double;
    for n in &nums {
        if n.class.is_int() {
            class = n.class;
            break;
        }
        if n.class == NumClass::Single {
            class = NumClass::Single;
        }
    }
    let parts: Vec<(&[usize], &[f64])> = nums
        .iter()
        .map(|n| (n.dims.as_slice(), n.re.as_slice()))
        .collect();
    let re = cat_data(&parts, d);
    let any_c = nums.iter().any(|n| n.im.is_some());
    let mut out = if any_c {
        let ims: Vec<Vec<f64>> = nums
            .iter()
            .map(|n| n.im.clone().unwrap_or_else(|| vec![0.0; n.re.len()]))
            .collect();
        let parts: Vec<(&[usize], &[f64])> = nums
            .iter()
            .zip(&ims)
            .map(|(n, im)| (n.dims.as_slice(), im.as_slice()))
            .collect();
        Num::complex(od, re, cat_data(&parts, d))
    } else {
        Num::real(od, re)
    };
    debug_assert_eq!(numel_of(&out.dims), out.re.len());
    out = out.with_class(class);
    Ok(Value::Num(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn horz_vert() {
        let a = Value::row(vec![1.0, 2.0]);
        let b = Value::row(vec![3.0]);
        let r = cat(vec![a.clone(), b], 1).unwrap();
        assert_eq!(r.to_f64_vec().unwrap(), vec![1.0, 2.0, 3.0]);
        let r = cat(vec![a.clone(), a], 0).unwrap();
        assert_eq!(r.dims(), vec![2, 2]);
        assert_eq!(r.to_f64_vec().unwrap(), vec![1.0, 1.0, 2.0, 2.0]);
    }
    #[test]
    fn char_and_empty() {
        let r = cat(vec![Value::str("ab"), Value::empty(), Value::str("c")], 1).unwrap();
        assert_eq!(r.to_str().unwrap(), "abc");
        assert!(cat(vec![Value::str("ab"), Value::str("c")], 0).is_err());
    }
}
