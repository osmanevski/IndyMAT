function [y,f] = uy_yap_yerel(x)
y=kare(x); f=@kare;
end
function y=kare(x)
y=x*x;
end
