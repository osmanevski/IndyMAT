function [artir,oku] = uy_yap_icice(x)
artir=@ekle; oku=@deger;
function y=ekle(k)
x=x+k; y=x;
end
function y=deger()
y=x;
end
end
