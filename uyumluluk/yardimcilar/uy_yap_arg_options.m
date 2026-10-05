function y=uy_yap_arg_options(x,options)
arguments
x
options.Scale (1,1) double = 2
options.Offset (1,1) double = 0
end
y=x*options.Scale+options.Offset;
end
