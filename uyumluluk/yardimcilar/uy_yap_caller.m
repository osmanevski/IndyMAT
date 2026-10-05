function y=uy_yap_caller(mode)
if strcmp(mode,'set')
assignin('caller','uy_yap_caller_deger',17); y=0;
else
y=evalin('caller','uy_yap_caller_deger+1');
end
end
