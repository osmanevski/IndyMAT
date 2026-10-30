//! Nested functions share one workspace with their parent (MATLAB rules):
//! a variable used in the parent and in a nested function is the same
//! variable, whichever path reaches the nested function; any other
//! variable of a nested function is local to it.

use indy_interp::Interp;
use std::path::PathBuf;

/// Write `src` as `<name>.m` into a fresh directory and run `call` with
/// that directory on the path; the file's own `assert`s check semantics.
fn run_fn(name: &str, src: &str, call: &str) -> Interp {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("nested-{name}"));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{name}.m")), src).unwrap();
    let mut it = Interp::new();
    it.add_path(dir, true);
    if let Err(e) = it.run_text(call) {
        panic!("{name} failed: {}\n---\n{}", e.message, src);
    }
    it
}

#[test]
fn sibling_calls_write_parent_workspace() {
    run_fn(
        "nf_sibling",
        "function nf_sibling
  items = {};
  top();
  assert(numel(items) == 3);
  assert(isequal(items, {1, 2, 3}));
  function add(v)
    items{end+1} = v;
  end
  function top()
    add(1);
    inner();
    assert(numel(items) == 3);
  end
  function inner()
    add(2); add(3);
  end
end
",
        "nf_sibling;",
    );
}

#[test]
fn handle_while_parent_runs_and_through_other_functions() {
    run_fn(
        "nf_handle",
        "function nf_handle
  n = 0;
  h = @bump;
  h(1);
  cellfun(@bump, {2, 3});
  arrayfun(h, [4 5]);
  nf_handle_apply(h, 10);
  assert(n == 25);
  function bump(k)
    n = n + k;
  end
end
function nf_handle_apply(f, v)
  f(v);
end
",
        "nf_handle;",
    );
}

#[test]
fn handle_keeps_workspace_after_parent_returns() {
    let it = run_fn(
        "nf_counter",
        "function [inc, get, push] = nf_counter()
  cnt = 0;
  acc = [];
  inc = @step; get = @value; push = @append;
  function step(), cnt = cnt + 1; end
  function v = value(), v = cnt; end
  function append(x), acc(end+1) = x; cnt = numel(acc); end
end
",
        "[i, g, p] = nf_counter(); i(); i(); a = g(); p(7); p(8); b = g();
         [i2, g2] = nf_counter(); i2(); c = g2(); d = g();",
    );
    let num = |n: &str| it.get_var(n).unwrap().to_f64_vec().unwrap();
    assert_eq!(num("a"), vec![2.0]);
    // `append` sets cnt from the shared acc, which saw both pushes.
    assert_eq!(num("b"), vec![2.0]);
    // A second call of the parent gets its own workspace.
    assert_eq!(num("c"), vec![1.0]);
    assert_eq!(num("d"), vec![2.0]);
}

#[test]
fn recursion_and_outputs() {
    run_fn(
        "nf_rec",
        "function nf_rec
  trail = [];
  r = fact(5);
  assert(r == 120);
  assert(isequal(trail, [1 2 3 4 5]));
  function r = fact(k)
    if k <= 1
      r = 1;
    else
      r = k * fact(k - 1);
    end
    trail(end+1) = k;
  end
end
",
        "nf_rec;",
    );
}

#[test]
fn scoping_locals_params_and_levels() {
    run_fn(
        "nf_scope",
        "function nf_scope
  z = 1; x = 100;
  mid(7);
  assert(z == 22 && x == 100);
  only_nested();
  assert(~exist('loc', 'var'));
  only_nested();
  function mid(x)
    y = 2;
    leaf();
    assert(x == 8 && y == 3);
    function leaf()
      z = z + 10;   % grandparent variable, not used in mid
      y = y + 1;    % mid's variable
      x = x + 1;    % mid's parameter shadows the parent's x
      sib();        % sibling of mid, reached from a grandchild
    end
  end
  function sib()
    z = z * 2;
  end
  function only_nested()
    assert(~exist('loc', 'var'));   % local: fresh on every call
    loc = 1;
  end
end
",
        "nf_scope;",
    );
}

#[test]
fn errors_keep_writes_made_before_them() {
    run_fn(
        "nf_err",
        "function nf_err
  s = 0;
  try
    boom();
  catch
  end
  assert(s == 1);
  function boom()
    s = 1;
    error('x:y', 'stop');
  end
end
",
        "nf_err;",
    );
}
