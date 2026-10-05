function uy_yap_rethrow()
try
uy_yap_error_leaf();
catch e
rethrow(e);
end
end
function uy_yap_error_leaf()
error('UyYap:Leaf','Sorun');
end
