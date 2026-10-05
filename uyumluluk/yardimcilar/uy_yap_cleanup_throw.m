function uy_yap_cleanup_throw(p)
c=onCleanup(@()delete(p));
error('UyYap:Cleanup','Sorun');
end
