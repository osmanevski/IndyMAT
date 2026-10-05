function clean=__mf_graphics_snapshot_props__(props,writable)
% Rendering properties only: never carry callbacks, user state or external handles.
  clean=struct();
  excluded={'parent','children','type','beingdeleted','userdata','uicontextmenu','contextmenu',...
    'xlabel','ylabel','zlabel','title','currentpoint','currentaxes','positionconstraint','outerposition'};
  for name=fieldnames(props)'
    key=name{1}; lowerkey=lower(key);
    if ~any(strcmp(key,writable)) || any(strcmp(lowerkey,excluded)) || ...
        strncmp(lowerkey,'__',2) || strncmp(lowerkey,'default',7) || ...
        ~isempty(regexp(lowerkey,'fcn$|callback','once'))
      continue;
    endif
    clean.(key)=props.(key);
  endfor
endfunction
