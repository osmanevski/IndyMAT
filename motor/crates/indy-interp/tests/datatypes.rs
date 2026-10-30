//! Tests for the native datatypes: table, datetime, duration,
//! calendarDuration, categorical and the classic date functions.

use indy_interp::Interp;

fn run(src: &str) -> Interp {
    let mut it = Interp::new();
    if let Err(e) = it.run_text(src) {
        panic!("{}\n  -> {} ({})", src, e.message, e.identifier);
    }
    it
}

fn text(it: &Interp, name: &str) -> String {
    it.get_var(name)
        .unwrap_or_else(|| panic!("no variable {}", name))
        .to_str()
        .unwrap()
}

fn fails(src: &str) -> String {
    let mut it = Interp::new();
    match it.run_text(src) {
        Ok(_) => panic!("{} should fail", src),
        Err(e) => e.message,
    }
}

#[test]
fn table_construction_and_access() {
    run(
        "T = table([1;2;3], {'a';'b';'c'}, 'VariableNames', {'n','s'});
         assert(istable(T) && isequal(size(T), [3 2]) && height(T) == 3 && width(T) == 2);
         assert(isequal(T.n, [1;2;3]) && isequal(T.s, {'a';'b';'c'}));
         assert(isequal(T.Properties.VariableNames, {'n','s'}));
         assert(isequal(T{2,'n'}, 2) && isequal(T{end,1}, 3));
         S = T(T.n > 1, :); assert(height(S) == 2 && isequal(S.s, {'b';'c'}));
         S = T(:, 's'); assert(width(S) == 1);
         x = [1;2]; y = [3;4]; U = table(x, y);
         assert(isequal(U.Properties.VariableNames, {'x','y'}));
         assert(isequal(table(x, x+1).Properties.VariableNames, {'x','Var2'}));",
    );
}

#[test]
fn table_assignment_growth_deletion() {
    run("T = table([1;2], 'VariableNames', {'a'});
         T.b = T.a * 10; assert(isequal(T.b, [10;20]));
         T.c = 0; assert(isequal(T.c, [0;0]));
         T.a(2) = 5; assert(isequal(T.a, [1;5]));
         T{3, 'a'} = 7; assert(height(T) == 3 && T.b(3) == 0);
         T(2, :) = []; assert(isequal(T.a, [1;7]));
         T(:, 'c') = []; assert(width(T) == 2);
         T(end+1, :) = {9, 90}; assert(isequal(T.a, [1;7;9]) && T.b(end) == 90);
         T.Properties.VariableNames{2} = 'bb'; assert(isequal(T.Properties.VariableNames, {'a','bb'}));
         T.Properties.Description = 'd'; assert(strcmp(T.Properties.Description, 'd'));
         T.Properties.RowNames = {'r1';'r2';'r3'}; assert(T{'r2','a'} == 7);");
    let m = fails("T = table([1;2]); T.x = [1;2;3];");
    assert!(m.contains("number of rows"), "{}", m);
    let m = fails("T = table([1;2]); T(1)");
    assert!(m.contains("linear indexing"), "{}", m);
    let m = fails("T = table([1;2]); T.nope");
    assert!(m.contains("nope"), "{}", m);
}

#[test]
fn table_conversions() {
    run("T = array2table([1 2; 3 4], 'VariableNames', {'x','y'});
         assert(isequal(table2array(T), [1 2; 3 4]));
         s = table2struct(T); assert(isequal(size(s), [2 1]) && s(2).y == 4);
         s = table2struct(T, 'ToScalar', true); assert(isequal(s.x, [1;3]));
         U = struct2table(struct('p', {1; 2}, 'q', {'a'; 'b'}));
         assert(isequal(U.p, [1;2]) && isequal(U.q, {'a';'b'}));
         V = cell2table({1, 'a'; 2, 'b'}, 'VariableNames', {'n','h'});
         assert(isequal(table2cell(V), {1, 'a'; 2, 'b'}));
         W = [T; T]; assert(height(W) == 4);
         X = [T, table([5;6], 'VariableNames', {'z'})]; assert(width(X) == 3);");
}

#[test]
fn table_functions() {
    run("T = table([3;1;2], {'c';'a';'b'}, 'VariableNames', {'k','h'});
         S = sortrows(T, 'k'); assert(isequal(S.k, [1;2;3]) && isequal(S.h, {'a';'b';'c'}));
         S = sortrows(T, 'k', 'descend'); assert(isequal(S.k, [3;2;1]));
         S = sortrows(T, 'h'); assert(isequal(S.k, [1;2;3]));
         R = table([1;2], [100;200], 'VariableNames', {'k','v'});
         J = innerjoin(T, R); assert(isequal(J.k, [1;2]) && isequal(J.v, [100;200]));
         O = outerjoin(T, R, 'MergeKeys', true); assert(height(O) == 3 && isnan(O.v(3)));
         T = addvars(T, [7;8;9], 'NewVariableNames', 'z', 'Before', 'k');
         assert(isequal(T.Properties.VariableNames, {'z','k','h'}));
         T = renamevars(T, 'z', 'w'); T = removevars(T, 'h'); assert(width(T) == 2);
         T = movevars(T, 'k', 'Before', 'w'); assert(isequal(T.Properties.VariableNames, {'k','w'}));
         G = table({'x';'y';'x'}, [1;2;3], 'VariableNames', {'g','v'});
         M = groupsummary(G, 'g', {'mean','sum'}, 'v');
         assert(isequal(M.mean_v, [2;2]) && isequal(M.sum_v, [4;2]) && isequal(M.GroupCount, [2;1]));
         V = varfun(@mean, table([1;2], [3;5], 'VariableNames', {'a','b'}));
         assert(V.mean_a == 1.5 && V.mean_b == 4);
         F = rowfun(@(a, b) a + b, table([1;2], [3;5]), 'OutputVariableNames', 'c');
         assert(isequal(F.c, [4;7]));
         H = table((1:10)'); assert(height(head(H, 3)) == 3 && isequal(tail(H, 2).Var1, [9;10]));
         assert(height(head(H)) == 8);
         assert(isequal(table([1;2]) + 1, table([2;3])));");
}

#[test]
fn table_display() {
    let it = run("T = table([1;2], {'a';'bb'}, 'VariableNames', {'n','s'}); t = evalc('disp(T)');");
    let t = text(&it, "t");
    assert!(t.contains("n      s"), "{}", t);
    assert!(t.contains("{'bb'}"), "{}", t);
    let it = run("T = table(string({'x';'y'}), [1.5; 22]); t = evalc('T');");
    let t = text(&it, "t");
    assert!(t.contains("2×2 table"), "{}", t);
    assert!(t.contains("\"x\""), "{}", t);
}

#[test]
fn datetime_basics() {
    run("d = datetime(2024, 1, 31);
         assert(isdatetime(d) && year(d) == 2024 && month(d) == 1 && day(d) == 31);
         assert(strcmp(char(d), '31-Jan-2024'));
         e = d + days(2); assert(month(e) == 2 && day(e) == 2);
         f = d + calmonths(1); assert(month(f) == 2 && day(f) == 29);
         g = d + 1; assert(day(g) == 1 && month(g) == 2);
         x = datetime(2024, 3, 1, 12, 0, 0) - datetime(2024, 3, 1, 6, 30, 0);
         assert(isduration(x) && hours(x) == 5.5 && strcmp(char(x), '05:30:00'));
         t = [datetime(2024, 6, 1) d]; assert(isequal(sort(t), [d datetime(2024, 6, 1)]));
         assert(t(2) < t(1) && ~(t(1) == t(2)) && t(1) == '2024-06-01');
         d = datetime('2024-03-05', 'InputFormat', 'yyyy-MM-dd');
         assert(strcmp(d.Format, 'yyyy-MM-dd'));
         d.Format = 'dd.MM.yyyy'; assert(strcmp(char(d), '05.03.2024'));
         d = datetime('05-Mar-2024 14:07:09'); assert(hour(d) == 14 && second(d) == 9);
         assert(strcmp(char(d, 'eeee'), 'Tuesday'));
         assert(isnat(NaT) && isequal(isnat([datetime(2024,1,1) NaT]), [false true]));
         assert(isequal(size(NaT(2, 3)), [2 3]));
         d = dateshift(datetime(2024, 3, 15, 10, 0, 0), 'start', 'month'); assert(day(d) == 1 && hour(d) == 0);
         d = dateshift(datetime(2024, 2, 10), 'end', 'month'); assert(day(d) == 29);
         [y, m, dd] = ymd(datetime(2020, 2, 29)); assert(y == 2020 && m == 2 && dd == 29);
         assert(posixtime(datetime(1970, 1, 2)) == 86400);
         assert(datenum(datetime(2000, 1, 1)) == 730486);
         d = datetime(730486.5, 'ConvertFrom', 'datenum'); assert(hour(d) == 12);
         d = datetime([2024 1 2; 2025 3 4]); assert(isequal(size(d), [2 1]) && year(d(2)) == 2025);
         d.Year = 2000; assert(all(year(d) == 2000));
         n = datetime('now'); assert(year(n) >= 2024);
         assert(isequal(day(datetime(2024,3,5), 'dayofweek'), 3));");
    let m = fails("datetime('garbage text')");
    assert!(m.contains("InputFormat"), "{}", m);
    let m = fails("d = datetime(2024,1,1) + datetime(2024,1,2);");
    assert!(m.contains("datetime"), "{}", m);
}

#[test]
fn durations_and_calendar() {
    run("assert(minutes(hours(2)) == 120 && hours(duration(2, 0, 0)) == 2);
         d = seconds(90); assert(strcmp(char(d), '90 sec'));
         d.Format = 'mm:ss'; assert(strcmp(char(d), '01:30'));
         assert(strcmp(char(days(2)), '2 days') && strcmp(char(hours(1)), '1 hr'));
         x = hours([1 2 3]); assert(isequal(hours(sum(x)), 6) && isequal(hours(mean(x)), 2));
         assert(hours(x(2) * 3) == 6 && x(3) / x(1) == 3 && hours(-x(1)) == -1);
         c = calendarDuration(1, 2, 3); assert(iscalendarduration(c) && strcmp(char(c), '1y 2mo 3d'));
         [m, dd] = split(c, {'months', 'days'}); assert(m == 14 && dd == 3);
         assert(calmonths(c) == 14);
         c2 = c + calmonths(1); assert(strcmp(char(c2), '1y 3mo 3d'));
         b = between(datetime(2024,1,1), datetime(2024,3,15));
         assert(strcmp(char(b), '2mo 14d'));");
}

#[test]
fn classic_dates() {
    run(
        "assert(strcmp(datestr(datenum(2024, 3, 5), 'yyyy-mm-dd'), '2024-03-05'));
         assert(isequal(datevec('2024-03-05', 'yyyy-mm-dd'), [2024 3 5 0 0 0]));
         assert(datenum(2000, 1, 1) == 730486 && datenum('01-Jan-2000') == 730486);
         assert(weekday(datenum(2024, 3, 5)) == 3);
         assert(eomday(2024, 2) == 29 && eomday(2023, 2) == 28);
         assert(strcmp(datestr(730486.5), '01-Jan-2000 12:00:00'));
         assert(strcmp(datestr(730486), '01-Jan-2000'));
         assert(strcmp(datestr(730486, 29), '2000-01-01'));
         [y, m] = datevec(730486); assert(y == 2000 && m == 1);
         assert(addtodate(730486, 1, 'month') == datenum(2000, 2, 1));",
    );
}

#[test]
fn categorical_basics() {
    run("c = categorical({'a', 'b', 'a', 'c'});
         assert(iscategorical(c) && isequal(categories(c), {'a'; 'b'; 'c'}));
         assert(isequal(countcats(c), [2 1 1]) && isequal(c == 'a', [true false true false]));
         assert(isequal(c ~= 'a', [false true false true]));
         c(2) = 'd'; assert(isequal(categories(c), {'a'; 'b'; 'c'; 'd'}));
         c = removecats(c); assert(isequal(categories(c), {'a'; 'c'; 'd'}));
         x = categorical([1 NaN 2], [1 2 3], {'lo', 'mid', 'hi'});
         assert(isequal(isundefined(x), [false true false]) && isequaln(double(x), [1 NaN 2]));
         o = categorical({'lo', 'hi'}, {'lo', 'hi'}, 'Ordinal', true);
         assert(isordinal(o) && o(1) < o(2));
         u = [categorical({'a'}) categorical({'b'})]; assert(isequal(categories(u), {'a'; 'b'}));
         assert(isequal(cellstr(u), {'a', 'b'}));
         T = table(categorical({'x'; 'y'; 'x'}), [1; 2; 3], 'VariableNames', {'g', 'v'});
         G = groupsummary(T, 'g', 'sum', 'v'); assert(isequal(G.sum_v, [4; 2]));");
    let m = fails("c = categorical({'a','b'}); c < c");
    assert!(m.contains("ordinal"), "{}", m);
}

#[test]
fn datatypes_package_and_exist() {
    run("pkg load datatypes; assert(exist('string', 'file') == 2 && exist('table') == 2);");
    let m = fails("pkg load nosuchpackage");
    assert!(m.contains("nosuchpackage"), "{}", m);
}

#[test]
fn bad_inputs_do_not_panic() {
    let cases = [
        "table(1, 'VariableNames', {'a', 'b'})",
        "table([1;2], [1;2;3])",
        "T = table(1); T{5, 7}",
        "T = table(1); T(1, 2, 3)",
        "T = table(1); T.Properties.Bogus = 1;",
        "sortrows(table({1;'a'}), 1)",
        "datetime(1, 2)",
        "datetime({1})",
        "datetime('2024-01-01', 'InputFormat', 'qqq')",
        "d = datetime(2024,1,1); d.Format = 5;",
        "d = datetime(2024,1,1); d{1}",
        "duration('x')",
        "seconds({1})",
        "calendarDuration(1.5, 0, 0)",
        "categorical({1, 2})",
        "c = categorical({'a'}, {'a'}, 'Protected', true); c(2) = 'b';",
        "datestr(1, 99)",
        "datevec('not a date')",
        "dateshift(datetime(2024,1,1), 'start', 'fortnight')",
        "datetime(2024,1,1) * 2",
        "hours(1) * hours(1)",
        "between(1, 2)",
    ];
    for c in cases {
        let mut it = Interp::new();
        assert!(it.run_text(c).is_err(), "{} should fail", c);
    }
    let ok = [
        "x = table();",
        "x = table('Size', [2 2], 'VariableTypes', {'double', 'string'});",
        "x = datetime(NaN, 1, 1);",
        "x = datetime(Inf, 1, 1);",
        "x = [datetime(2024,1,1); NaT];",
        "x = categorical([]);",
        "x = hours([]);",
        "x = sort(datetime([2024 1 1; 2023 1 1]));",
        "T = table([1;2]); T(:, :) = [];",
        "T = table([1;2]); x = T([], :);",
    ];
    for c in ok {
        let mut it = Interp::new();
        if let Err(e) = it.run_text(c) {
            panic!("{} -> {}", c, e.message);
        }
    }
}

#[test]
fn readtable_detects_datetime_columns() {
    let dir = std::env::temp_dir().join(format!("indy-dt-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("t.csv");
    let src = format!(
        "T = table([datetime(2024,1,1); datetime(2024,2,1)], [1; 2], 'VariableNames', {{'t','v'}});
         writetable(T, '{0}'); R = readtable('{0}');
         assert(isdatetime(R.t) && isequal(R.t, T.t) && isequal(R.v, [1; 2]));",
        f.display()
    );
    run(&src);
    let _ = std::fs::remove_dir_all(&dir);
}
