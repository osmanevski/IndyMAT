% MATLAB R2025b olc26-object-next: exact component reconstruction.
%% wave26-calendar-components | calendarDuration split
x=calendarDuration([1 0],[2 -3],[4 5],[6 7],[8 9],[10.125 11.5]);
[m,d,s]=split(x,{'months','days','time'}); s=seconds(s);
assert(isequal(m,[14 -3])&&isequal(d,[4 5])&&isequal(s,[22090.125 25751.5]));
y=calendarDuration(zeros(size(m)),m,d,zeros(size(m)),zeros(size(m)),s);
y.Format=x.Format; assert(isequal(x,y));
