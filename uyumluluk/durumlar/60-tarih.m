% Tarih ve saat
%% tarih-olusturma | datetime year month day isdatetime
d = datetime(2024, 1, 31);
assert(isdatetime(d));
assert(year(d) == 2024 && month(d) == 1 && day(d) == 31);
%% tarih-sure-aritmetigi | datetime days hours duration
d = datetime(2024, 2, 28) + days(2);
assert(month(d) == 3 && day(d) == 1);
fark = datetime(2024, 3, 1, 12, 0, 0) - datetime(2024, 3, 1, 6, 0, 0);
assert(isduration(fark) && hours(fark) == 6);
assert(minutes(hours(2)) == 120);
assert(hours(duration(2, 0, 0)) == 2);
%% tarih-takvim-aritmetigi | calmonths calendarDuration iscalendarduration
d = datetime(2024, 1, 31) + calmonths(1);
assert(month(d) == 2 && day(d) == 29);
e = datetime(2024, 1, 31) + calendarDuration(0, 1, 0);
assert(month(e) == 2 && day(e) == 29);
assert(iscalendarduration(calendarDuration(0, 1, 0)));
%% tarih-karsilastirma | datetime
a = datetime(2024, 1, 1); b = datetime(2024, 6, 1);
assert(a < b && ~(a == b));
t = [b a];
assert(isequal(sort(t), [a b]));
%% tarih-metin-bicimi | datetime
d = datetime('2024-03-05', 'InputFormat', 'yyyy-MM-dd');
assert(day(d) == 5 && month(d) == 3);
d.Format = 'dd.MM.yyyy';
assert(strcmp(char(d), '05.03.2024'));
%% tarih-dateshift | dateshift
d = dateshift(datetime(2024, 3, 15), 'start', 'month');
assert(day(d) == 1 && month(d) == 3);
%% tarih-nat | NaT isnat
assert(isnat(NaT));
assert(isequal(isnat([datetime(2024, 1, 1) NaT]), [false true]));
%% tarih-klasik | datestr datenum datevec weekday eomday
assert(strcmp(datestr(datenum(2024, 3, 5), 'yyyy-mm-dd'), '2024-03-05'));
assert(isequal(datevec('2024-03-05', 'yyyy-mm-dd'), [2024 3 5 0 0 0]));
assert(datenum(2000, 1, 1) == 730486);
assert(weekday(datenum(2024, 3, 5)) == 3);
assert(eomday(2024, 2) == 29);
