% Real MATLAB R2025b olcum-inc2-matlab.txt: measured extreme magnitudes.
%% wave26-norm-scaled-negative | norm
xs={[1e200 1e200],[1e-200 1e-200],[1e308 1e308]}; ps=[-2 -2 -1]; es=[7.071067811865474e199 7.071067811865475e-201 5e307];
for k=1:3, v=norm(xs{k},ps(k)); assert(isfinite(v)&&abs(v/es(k)-1)<5e-15); end
%% wave26-norm-native-positive-three | norm
v=norm([1e200 1e200],3); assert(isfinite(v)&&abs(v/1.259921049894873e200-1)<5e-15);
%% wave26-norm-native-negative-three | norm
v=norm([1e-200 1e-200],-3); assert(isfinite(v)&&abs(v/7.937005259840998e-201-1)<5e-15);
