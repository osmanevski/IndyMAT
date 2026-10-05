function y=uy_yap_arg_repeat(x,k)
arguments (Repeating)
x (1,1) double
k (1,1) double
end
y=sum([x{:}].*[k{:}]);
end
