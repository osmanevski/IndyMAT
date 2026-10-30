//! Object arrays of classdef instances.

use indy_interp::Interp;
use std::path::PathBuf;

fn run(code: &str) {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("objarr");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("OaPt.m"),
        "classdef OaPt
  properties
    X = 0
    Y = 0
  end
  methods
    function o = OaPt(x, y)
      if nargin > 0, o.X = x; o.Y = y; end
    end
    function d = total(o)
      d = sum([o.X]) + sum([o.Y]);
    end
  end
end
",
    )
    .unwrap();
    std::fs::write(
        dir.join("OaH.m"),
        "classdef OaH < handle
  properties
    V = 1
  end
end
",
    )
    .unwrap();
    let mut it = Interp::new();
    it.add_path(dir, true);
    if let Err(e) = it.run_text(code) {
        panic!("snippet failed: {}\n---\n{}", e.message, code);
    }
}

#[test]
fn build_index_and_copy() {
    run("a(1) = OaPt(1, 2); a(2) = OaPt(3, 4);
         assert(isequal(size(a), [1 2]) && numel(a) == 2 && ~isempty(a));
         assert(isa(a, 'OaPt') && strcmp(class(a), 'OaPt'));
         assert(isequal([a.X], [1 3]) && isequal([a.Y], [2 4]));
         b = a; b(1).X = 9;
         assert(a(1).X == 1 && b(1).X == 9);
         x = a(end); assert(x.X == 3);
         s = a(1:2); assert(isequal(size(s), [1 2]));
         assert(total(a) == 10);");
}

#[test]
fn growing_fills_default_objects() {
    run("c(3) = OaPt(5, 5);
         assert(isequal([c.X], [0 0 5]));
         h(2) = OaH; h(1).V = 5;
         assert(isequal([h.V], [5 1]));");
}

#[test]
fn empty_concat_delete_arrayfun() {
    run("e = OaPt.empty(0, 2);
         assert(isequal(size(e), [0 2]) && isempty(e) && isa(e, 'OaPt'));
         z = OaPt.empty; assert(isequal(size(z), [0 0]));
         e(1) = OaPt(7, 7); assert(isequal(size(e), [1 1]) && e.X == 7);
         q = [OaPt(1, 1) OaPt(2, 2) OaPt(3, 3)];
         assert(isequal([q.X], [1 2 3]));
         q(2) = []; assert(isequal([q.X], [1 3]));
         r = arrayfun(@(p) p.X * 10, q); assert(isequal(r, [10 30]));
         c = arrayfun(@(k) OaPt(k, 0), 1:2, 'UniformOutput', false);
         assert(iscell(c) && c{2}.X == 2);
         try
           q(1) = 5; ok = false;
         catch
           ok = true;
         end
         assert(ok);");
}
