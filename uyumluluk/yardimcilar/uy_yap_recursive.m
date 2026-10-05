function y=uy_yap_recursive(n)
if n<=1, y=1; else, y=n*uy_yap_recursive(n-1); end
end
