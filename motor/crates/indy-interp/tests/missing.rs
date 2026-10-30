//! Data cleaning builtins: ismissing, standardizeMissing, rmmissing,
//! fillmissing, isoutlier, rmoutliers, filloutliers, smoothdata.
//!
//! GNU Octave 11 (no packages) has none of these functions, so reference
//! values are computed by hand from the MathWorks documentation
//! definitions, except where noted: the GESD case uses the NIST/SEMATECH
//! e-Handbook example (Rosner data, 3 outliers, lambda_4 = 3.136) and the
//! smoothdata cases are copied from
//! `uyumluluk/durumlar/31-sayisal-yardimci.m` (sayisaly-smoothdata-*).

use indy_interp::Interp;

fn run(src: &str) {
    let mut it = Interp::new();
    if let Err(e) = it.run_text(src) {
        panic!("{}\n  -> {} ({})", src, e.message, e.identifier);
    }
}

fn fails(src: &str, needle: &str) {
    let mut it = Interp::new();
    match it.run_text(src) {
        Ok(_) => panic!("expected an error from: {}", src),
        Err(e) => assert!(
            e.message.contains(needle),
            "{}\n  -> '{}' does not contain '{}'",
            src,
            e.message,
            needle
        ),
    }
}

#[test]
fn scoreboard_cases() {
    run(r#"assert(isequal(rmmissing([1 NaN 2]), [1 2]));
        assert(isequal(fillmissing([1 NaN 3], 'linear'), [1 2 3]));
        assert(isequal(fillmissing([1 NaN 3], 'constant', 0), [1 0 3]));
        assert(isequal(ismissing([1 NaN 3]), [false true false]));
        assert(isequal(isoutlier([1 2 2 3 2 100]), [false false false false false true]));"#);
}

#[test]
fn ismissing_standard_and_indicators() {
    run(
        r#"assert(isequal(ismissing([1 NaN; NaN 2]), logical([0 1; 1 0])));
        assert(isequal(ismissing(complex(1, NaN)), true));
        assert(isequal(ismissing(int8([1 2])), [false false]));
        assert(isequal(ismissing([true false]), [false false]));
        assert(isequal(ismissing('a b'), [false true false]));
        assert(isequal(ismissing({'a', '', 'c'}), [false true false]));
        assert(isequal(ismissing({1, ''}), [false true]));
        assert(isequal(ismissing([string('a') missing]), [false true]));
        s.a = 1; assert(isequal(ismissing(s), false));
        % an explicit indicator replaces the standard missing values
        assert(isequal(ismissing([1 -99 NaN], -99), [false true false]));
        assert(isequal(ismissing([1 -99 NaN], [-99 NaN]), [false true true]));
        assert(isequal(ismissing(int8([1 0 3]), 0), [false true false]));
        assert(isequal(ismissing('ab c', 'b'), [false true false false]));
        assert(isequal(ismissing({'a', 'NA', ''}, 'NA'), [false true false]));
        assert(isequal(ismissing([string('a') "NA" missing], {'NA'}), [false true false]));
        assert(isequal(ismissing([1 -99 5], {-99, 'NA'}), [false true false]));"#,
    );
    fails("ismissing([1 2], 'a')", "indicators must be numeric");
}

#[test]
fn standardize_missing() {
    run(
        r#"assert(isequaln(standardizeMissing([1 -99 3], -99), [1 NaN 3]));
        x = standardizeMissing(single([0 1]), 0); assert(isa(x, 'single') && isnan(x(1)));
        assert(isequal(standardizeMissing({'a', 'NA'}, {'NA'}), {'a', ''}));
        assert(isequal(standardizeMissing('a-b', '-'), 'a b'));
        s = standardizeMissing([string('x') "NA"], 'NA'); assert(ismissing(s(2)) && s(1) == "x");"#,
    );
    fails("standardizeMissing(int8([1 2]), 2)", "no standard missing");
    fails("standardizeMissing(true, 1)", "no standard missing");
}

#[test]
fn rmmissing_all_forms() {
    run(r#"assert(isequal(rmmissing([1 NaN 3]), [1 3]));
        assert(isequal(rmmissing([1 2; NaN 3; 4 5]), [1 2; 4 5]));
        [r, tf] = rmmissing([1; NaN]); assert(isequal(r, 1) && isequal(tf, [false; true]));
        assert(isequal(rmmissing([1 NaN; 2 3], 2), [1; 2]));
        assert(isequaln(rmmissing([1 NaN; NaN NaN; 3 4], 'MinNumMissing', 2), [1 NaN; 3 4]));
        [r, tf] = rmmissing([string('a') missing "b"]); assert(isequal(r, ["a" "b"]) && isequal(tf, [false true false]));
        assert(isequal(rmmissing({'a', '', 'b'}), {'a', 'b'}));
        assert(isequal(rmmissing('a b'), 'ab'));
        assert(isequal(size(rmmissing([])), [0 0]));"#);
    fails("rmmissing(ones(2,2,2))", "vector or a matrix");
    fails("rmmissing([1 2], 3)", "dimension must be 1 or 2");
}

#[test]
fn fillmissing_basic_methods() {
    run(r#"x = [NaN 1 NaN NaN 4 NaN];
        [f, tf] = fillmissing(x, 'previous');
        assert(isequaln(f, [NaN 1 1 1 4 4]) && isequal(tf, logical([0 0 1 1 0 1])));
        [f, tf] = fillmissing(x, 'next');
        assert(isequaln(f, [1 1 4 4 4 NaN]) && isequal(tf, logical([1 0 1 1 0 0])));
        assert(isequal(fillmissing(x, 'nearest'), [1 1 1 4 4 4]));
        assert(isequal(fillmissing([1 NaN 3], 'nearest'), [1 3 3]));
        assert(max(abs(fillmissing(x, 'linear') - (0:5))) < 1e-12);
        assert(isequaln(fillmissing(x, 'linear', 'EndValues', 'none'), [NaN 1 2 3 4 NaN]));
        assert(isequal(fillmissing(x, 'linear', 'EndValues', 0), [0 1 2 3 4 0]));
        assert(isequal(fillmissing(x, 'linear', 'EndValues', 'nearest'), [1 1 2 3 4 4]));
        assert(isequaln(fillmissing(x, 'next', 'EndValues', 'previous'), [NaN 1 4 4 4 4]));
        % gaps are measured between the surrounding sample points
        assert(isequaln(fillmissing(x, 'linear', 'MaxGap', 2), [0 1 NaN NaN 4 5]));
        assert(isequal(fillmissing(x, 'linear', 'MaxGap', 3), [0 1 2 3 4 5]));
        assert(isequal(fillmissing([1 NaN 5 NaN 9 NaN], 'movmean', 3), [1 3 5 7 9 9]));
        assert(isequal(fillmissing([1 NaN 5 NaN 9 NaN], 'movmedian', [2 0]), [1 1 5 5 9 9]));
        % fewer than two data points: interpolation leaves the gap
        assert(isequaln(fillmissing([NaN 1 NaN], 'linear'), [NaN 1 NaN]));
        assert(isequaln(fillmissing([NaN NaN], 'linear'), [NaN NaN]));"#);
}

#[test]
fn fillmissing_interpolation_and_options() {
    run(
        r#"f = fillmissing([1 NaN 3 NaN], 'spline', 'SamplePoints', [1 2 4 8]);
        assert(max(abs(f - [1 5/3 3 17/3])) < 1e-12);
        y = [0 1 NaN 27 64 NaN]; f = fillmissing(y, 'spline');
        assert(max(abs(f - (0:5).^3)) < 1e-9);
        f = fillmissing([1 2 NaN 4 6], 'pchip'); assert(f(3) > 2 && f(3) < 4);
        f = fillmissing([1 2 NaN 4 6], 'makima'); assert(f(3) > 2 && f(3) < 4);
        x = [1 2 NaN 4 6]; assert(isequal(fillmissing(x, 'pchip'), [1 2 interp1([1 2 4 5], [1 2 4 6], 3, 'pchip') 4 6]));
        A = [1 NaN NaN 4; NaN 2 3 NaN];
        assert(isequal(fillmissing(A, 'constant', [7 8 9 10]), [1 8 9 4; 7 2 3 10]));
        assert(isequaln(fillmissing(A, 'previous', 2), [1 1 1 4; NaN 2 3 3]));
        assert(isequaln(fillmissing(A, 'previous'), [1 NaN NaN 4; 1 2 3 4]));
        f = fillmissing(single([1 NaN 3]), 'linear'); assert(isa(f, 'single') && isequal(f, single([1 2 3])));
        [f, tf] = fillmissing(int8([1 2 3]), 'linear'); assert(isa(f, 'int8') && ~any(tf));
        [f, tf] = fillmissing([1 5 3], 'constant', 0, 'MissingLocations', [false true false]);
        assert(isequal(f, [1 0 3]) && isequal(tf, [false true false]));
        f = fillmissing(int8([1 5 3]), 'linear', 'MissingLocations', [false true false]);
        assert(isa(f, 'int8') && isequal(f, int8([1 2 3])));
        f = fillmissing([1+2i NaN 3], 'linear'); assert(abs(f(2) - (2+1i)) < 1e-12);
        B = reshape([1 NaN 3 4 NaN 6 7 NaN], [2 2 2]); f = fillmissing(B, 'constant', -1, 3);
        assert(isequal(f(:), [1 -1 3 4 -1 6 7 -1]'));"#,
    );
}

#[test]
fn fillmissing_text() {
    run(
        r#"assert(isequal(fillmissing('a  b', 'constant', 'x'), 'axxb'));
        assert(isequal(fillmissing('a  b', 'previous'), 'aaab'));
        assert(isequal(fillmissing({'a', '', 'c'}, 'previous'), {'a', 'a', 'c'}));
        assert(isequal(fillmissing({'a', '', 'c'}, 'constant', 'z'), {'a', 'z', 'c'}));
        s = fillmissing([string('a') missing "c"], 'constant', 'z'); assert(isequal(s, ["a" "z" "c"]));
        s = fillmissing([string('a') missing "c"], 'next'); assert(isequal(s, ["a" "c" "c"]));
        [s, tf] = fillmissing([missing string('b')], 'previous'); assert(ismissing(s(1)) && ~tf(1));"#,
    );
    fails("fillmissing('a b', 'linear')", "requires numeric input");
    fails(
        "fillmissing({1, ''}, 'previous')",
        "cell array of character vectors",
    );
    fails("fillmissing([1 NaN], 'constant', 'x')", "not compatible");
    fails(
        "fillmissing([1 NaN; 2 NaN], 'constant', [1 2 3])",
        "one value per vector",
    );
    fails("fillmissing([1 NaN], 'foo')", "unknown fill method");
    fails("fillmissing([1 NaN], 'knn')", "not supported");
    fails(
        "fillmissing([1 NaN], 'linear', 'SamplePoints', [2 1])",
        "sorted",
    );
    fails("fillmissing([1 NaN], 'movmean')", "requires a window");
}

#[test]
fn isoutlier_static_methods() {
    run(r#"[tf, L, U, C] = isoutlier([1 2 2 3 2 100]);
        c = 1.482602218505602; m = 0.5*c;
        assert(isequal(tf, logical([0 0 0 0 0 1])) && C == 2);
        assert(abs(L - (2 - 3*m)) < 1e-12 && abs(U - (2 + 3*m)) < 1e-12);
        [tf, L, U, C] = isoutlier([1 2 2 3 2 100], 'mean');
        x = [1 2 2 3 2 100];
        assert(~any(tf) && abs(C - mean(x)) < 1e-12 && abs(U - (mean(x) + 3*std(x))) < 1e-10);
        tf = isoutlier([1 2 2 3 2 100], 'mean', 'ThresholdFactor', 2); assert(isequal(tf, logical([0 0 0 0 0 1])));
        % quartiles: prctile (midpoint) definition, 1.5 IQR
        [tf, L, U, C] = isoutlier([1 2 2 3 2 100 NaN], 'quartiles');
        assert(isequal(tf, logical([0 0 0 0 0 1 0])) && L == 0.5 && U == 4.5 && C == 2.5);
        % NaN is never an outlier and is ignored by the statistics
        assert(isequal(isoutlier([NaN 1 2 2 3 2 100]), logical([0 0 0 0 0 0 1])));
        % operates on columns of a matrix; dim 2 works along rows
        A = [1 1; 2 1; 2 50; 3 1; 2 1; 100 1];
        [tf, L] = isoutlier(A); assert(isequal(tf(:, 1)', logical([0 0 0 0 0 1])) && tf(3, 2) && isequal(size(L), [1 2]));
        tf = isoutlier(A', 2); assert(isequal(tf, isoutlier(A)'));
        [~, L] = isoutlier(single([1 2 3])); assert(isa(L, 'single'));"#);
    fails("isoutlier({1})", "numeric or logical");
    fails(
        "isoutlier([1 2 3], 'percentiles', [10 90])",
        "not supported",
    );
    fails(
        "isoutlier([1 2], 'grubbs', 'ThresholdFactor', 2)",
        "between 0 and 1",
    );
}

#[test]
fn isoutlier_tests_grubbs_gesd() {
    // NIST/SEMATECH e-Handbook, generalized ESD example (Rosner 1983 data).
    run(
        r#"x = [-0.25 0.68 0.94 1.15 1.20 1.26 1.26 1.34 1.38 1.43 1.49 1.49 1.55 1.56 1.58 1.65 1.69 1.70 1.76 1.77 1.81 1.91 1.94 1.96 1.99 2.06 2.09 2.10 2.14 2.15 2.23 2.24 2.26 2.35 2.37 2.40 2.47 2.54 2.62 2.64 2.90 2.92 2.92 2.93 3.21 3.26 3.30 3.59 3.68 4.30 4.64 5.34 5.42 6.01];
        [tf, L, U, C] = isoutlier(x, 'gesd', 'MaxNumOutliers', 10);
        assert(isequal(find(tf), [52 53 54]));
        y = x(~tf); assert(abs((U - mean(y))/std(y) - 3.136) < 1e-3 && abs(C - mean(y)) < 1e-12);
        % Grubbs: first statistic 3.118 is below the n = 54 critical value 3.158
        [tf, L, U] = isoutlier(x, 'grubbs'); assert(~any(tf) && abs((U - mean(x))/std(x) - 3.158) < 1e-3);
        % a clear single outlier (n = 10: two-sided critical value 2.290)
        z = [1 2 3 4 5 6 7 8 9 40]; tf = isoutlier(z, 'grubbs'); assert(isequal(find(tf), 10));
        [~, ~, U] = isoutlier(z(1:9), 'grubbs'); assert(~any(isoutlier(z(1:9), 'grubbs')));
        assert(isequal(find(isoutlier(z, 'gesd')), 10));"#,
    );
}

#[test]
fn isoutlier_moving() {
    run(r#"x = [1 2 3 50 4 5 6];
        [tf, L, U, C] = isoutlier(x, 'movmedian', 3);
        assert(isequal(find(tf), 4) && isequal(size(L), size(x)) && isequal(C, [1.5 2 3 4 5 5 5.5]));
        c = 1.482602218505602; assert(abs(U(2) - (2 + 3*c*1)) < 1e-12);
        [tf, L, U, C] = isoutlier([1 1 1 9 1 1 1], 'movmean', 5, 'ThresholdFactor', 1);
        assert(tf(4) && abs(C(4) - 13/5) < 1e-12);"#);
}

#[test]
fn rmoutliers_and_filloutliers() {
    run(r#"[B, tf] = rmoutliers([1 2; 2 3; 100 2; 3 4; 2 3]);
        assert(isequal(B, [1 2; 2 3; 3 4; 2 3]) && isequal(tf, logical([0 0 1 0 0])'));
        [B, tf] = rmoutliers([1 2 2 3 2 100]); assert(isequal(B, [1 2 2 3 2]) && isequal(tf, logical([0 0 0 0 0 1])));
        B = rmoutliers([1 2 2 3 2 100], 'mean'); assert(isequal(B, [1 2 2 3 2 100]));
        B = rmoutliers([1 1; 2 1; 100 50; 3 1; 2 1; 4 1], 'MinNumOutliers', 2); assert(isequal(size(B), [5 2]));
        x = [1 2 4 3 2 100 2 3 2 4];
        [B, tf, L, U, C] = filloutliers(x, 'linear');
        assert(isequal(find(tf), 6) && B(6) == 2 && C == median(x));
        assert(isequal(filloutliers(x, 'previous'), [1 2 4 3 2 2 2 3 2 4]));
        assert(isequal(filloutliers(x, 'next'), [1 2 4 3 2 2 2 3 2 4]));
        assert(isequal(filloutliers(x, 0), [1 2 4 3 2 0 2 3 2 4]));
        B = filloutliers(x, 'center'); assert(B(6) == median(x));
        [B, ~, L, U] = filloutliers(x, 'clip'); assert(B(6) == U);
        B = filloutliers([5 1 1 1 1], 'nearest', 'quartiles'); assert(isequal(B, [1 1 1 1 1]));
        B = filloutliers(single([1 2 2 3 2 100 2 3 2 3]), 'linear'); assert(isa(B, 'single'));"#);
    fails("filloutliers([1 2], 'foo')", "unknown fill method");
    fails("rmoutliers(ones(2,2,2))", "vector or a matrix");
}

#[test]
fn smoothdata_cases() {
    // copied from uyumluluk/durumlar/31-sayisal-yardimci.m (sayisaly-smoothdata-*)
    run(
        r#"v = smoothdata(1:5,'movmean',3); assert(max(abs(v-[1.5 2 3 4 4.5])) < 1e-12);
        v = smoothdata(1:5,'movmean',4); assert(max(abs(v-[1.5 2 2.5 3.5 4])) < 1e-12);
        v = smoothdata([1 100 3 4 5],'movmedian',3);
        assert(max(abs(v-[50.5 3 4 4 4.5])) < 1e-12);
        v = smoothdata([1 2 NaN 4 5],'movmean',3);
        assert(max(abs(v-[1.5 1.5 3 4.5 4.5])) < 1e-12);
        v = smoothdata([NaN NaN NaN 4],'movmedian',3); assert(isnan(v(1)) && isnan(v(2)) && v(3)==4);
        A = reshape(1:24,[2 3 4]); v = smoothdata(A,2,'movmean',3);
        e = v(:,2,:)-mean(A,2); assert(isequal(size(v),size(A)) && max(abs(e(:))) < 1e-12);
        [v,w] = smoothdata(single([1 2 3]),'movmean',[1 0]);
        assert(isa(v,'single') && isequal(w,[1 0]) && max(abs(double(v)-[1 1.5 2.5])) < 1e-6);
        q = exp(-25/18); v = smoothdata([0 0 1 0 0],'gaussian',3);
        assert(max(abs(v-[0 q 1 q 0]/(1+2*q))) < 1e-12);
        v = smoothdata([1 0 0],'gaussian',3); assert(abs(v(1)-1/(1+q)) < 1e-12);"#,
    );
}

#[test]
fn smoothdata_regression_methods() {
    run(r#"x = 1:8;
        % local polynomial fits reproduce polynomials of their degree
        assert(max(abs(smoothdata(x, 'lowess', 5) - x)) < 1e-10);
        assert(max(abs(smoothdata(x.^2, 'loess', 5) - x.^2)) < 1e-9);
        assert(max(abs(smoothdata(x.^2, 'sgolay', 5) - x.^2)) < 1e-9);
        assert(max(abs(smoothdata(x.^3, 'sgolay', 7, 'Degree', 3) - x.^3)) < 1e-8);
        assert(max(abs(smoothdata(x, 'rlowess', 5) - x)) < 1e-10);
        assert(max(abs(smoothdata(x.^2, 'rloess', 7) - x.^2)) < 1e-8);
        % sgolay degree 0 is the moving mean
        assert(max(abs(smoothdata([3 1 4 1 5 9 2], 'sgolay', 3, 'Degree', 0) - movmean([3 1 4 1 5 9 2], 3))) < 1e-12);
        v = smoothdata([1 2 NaN 4], 'movmean', 3, 'includenan'); assert(v(1) == 1.5 && all(isnan(v(2:4))));
        v = smoothdata(int8([1 2 3]), 'movmean', 3); assert(isa(v, 'double'));
        [v, w] = smoothdata([1 2 3 4], 'movmedian', 2); assert(w == 2 && isequal(v, [1 1.5 2.5 3.5]));"#);
    fails("smoothdata([1 2 3])", "automatic window selection");
    fails(
        "smoothdata([1 2 3], 'sgolay', 3, 'Degree', 3)",
        "less than the window",
    );
    fails("smoothdata([1 2 3], 'gaussian', [1 1])", "scalar window");
    fails(
        "smoothdata([1 2 3], 'movmean', 3, 'SmoothingFactor', 0.5)",
        "not supported",
    );
    fails("smoothdata({1}, 'movmean', 3)", "numeric or logical");
}

#[test]
fn never_panics_on_odd_inputs() {
    let cases = [
        "fillmissing([], 'linear')",
        "fillmissing(zeros(0,3), 'previous')",
        "fillmissing(NaN, 'linear')",
        "fillmissing(NaN(3), 'movmean', 1e300)",
        "fillmissing([1 NaN], 'movmedian', [1e20 1e20])",
        "fillmissing([1 NaN 3], 'linear', 'MaxGap', 0.5)",
        "isoutlier([])",
        "isoutlier(NaN(2,3))",
        "isoutlier([1 2], 'grubbs')",
        "isoutlier([1 1 1 1], 'gesd')",
        "isoutlier([1 2 3], 'movmedian', 1e300)",
        "isoutlier(zeros(3,0), 'movmean', 3)",
        "rmoutliers([])",
        "filloutliers([], 'linear')",
        "filloutliers(NaN(1,4), 'nearest')",
        "smoothdata([], 'movmean', 3)",
        "smoothdata(NaN(1,5), 'rloess', 3)",
        "smoothdata([1 Inf 3], 'lowess', 3)",
        "smoothdata(1:3, 'gaussian', 1e300)",
        "smoothdata(1:3, 'sgolay', [1e20 1e20])",
        "ismissing([])",
        "standardizeMissing([], 1)",
        "rmmissing({})",
    ];
    for c in cases {
        let mut it = Interp::new();
        let _ = it.run_text(&format!("x = {};", c));
    }
}
