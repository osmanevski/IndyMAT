function verify_dalga26_properties()
  % UNVERIFIED orchestrator probe. Run ONLY in an isolated graphics-capable
  % Octave session outside the lane sandbox. This prints native properties;
  % it does not synthesize a reference tree or claim pixel equivalence.
  cases={
    'contourf finite regions', "[C,h]=contourf([0 1 2;0 1 2],[.5 1.5]);";
    'contourf natural hole', "[C,h]=contourf([2 2 2;2 0 2;2 2 2],[.5 1.5]);";
    'contourf manual Z', "[C,h]=contourf([0 1 2;0 1 2],[.5 1.5]);set(h,'zlevelmode','manual','zlevel',7);view(3);";
    'clabel group rotation', "[C,h]=contour(peaks(9));clabel(C,h,'rotation',35,'fontsize',11,'color',[.2 .5 .8]);";
    'clabel matrix', "C=contour(peaks(9));clabel(C);";
    'clabel 3D', "[C,h]=contour3(peaks(9));clabel(C,h);";
    'patch indexed vertex colours', "h=patch('vertices',[0 0 1;1 0 1;0 1 1],'faces',[1 2 3],'facevertexcdata',[0;1;2],'facecolor','interp','edgecolor','interp');view(3);";
    'patch uint8 vertex colours', "h=patch('vertices',[0 0 1;1 0 1;0 1 1],'faces',[1 2 3],'facevertexcdata',uint8(255*eye(3)),'facecolor','interp','edgecolor','interp');view(3);";
    'patch quad interpolation diagonal', "h=patch('vertices',[0 0 1;1 0 1;1 1 1;0 1 1],'faces',[1 2 3 4],'facevertexcdata',[0;1;0;1],'facecolor','interp','edgecolor','none');colormap(gray(256));view(2);";
    'surface scalar alpha', "h=surf([0 0;0 0],'facealpha',.5,'edgealpha',.25);";
    'surface flat alpha', "h=surf([0 0;0 0],'alphadata',[0 .25;.75 1],'alphadatamapping','none','facealpha','flat');";
    'surface interp alpha', "h=surf([0 0;0 0],'alphadata',[0 .25;.75 1],'alphadatamapping','none','facealpha','interp');";
    'surface alpha direct uint8', "h=surf([0 0;0 0],'alphadata',uint8([0 1;2 3]),'alphadatamapping','direct','facealpha','interp');";
    'surface alpha scaled', "h=surf([0 0;0 0],'alphadata',[-1 .25;.75 2],'alphadatamapping','scaled','facealpha','interp');set(gca,'alim',[0 1]);";
    'patch scalar alpha', "h=patch('vertices',[0 0 1;1 0 1;0 1 1],'faces',[1 2 3],'facecolor','red','facealpha',.5,'edgealpha',.25);view(3);";
    'patch face alpha', "h=patch('vertices',[0 0 1;1 0 1;1 1 1;0 1 1],'faces',[1 2 3;1 3 4],'facecolor','red','facevertexalphadata',[.25;.75],'facealpha','flat');view(3);";
    'patch vertex alpha', "h=patch('vertices',[0 0 1;1 0 1;0 1 1],'faces',[1 2 3],'facecolor','red','facevertexalphadata',[0;.5;1],'facealpha','interp');view(3);";
    'unlit normals before draw', "h=surf([0 1;2 3]);";
    'flat normals and infinite light', "h=surf([0 1;2 3]);light('position',[1 0 1],'style','infinite');lighting flat;material dull;";
    'gouraud normals and local light', "h=surf([0 1;2 4]);light('position',[1 0 1],'style','local');lighting gouraud;material shiny;";
    'patch normals', "h=patch('vertices',[0 0 1;1 0 1;1 1 2;0 1 2],'faces',[1 2 3 4],'facecolor','red');light;lighting flat;view(3);"
  };
  for k=1:rows(cases)
    close all;f=figure('visible','off','name',['D26 properties: ',cases{k,1}]);
    fprintf('\nCASE %d %s\n',k,cases{k,1});
    try
      eval(cases{k,2});
      title(cases{k,1});
      fprintf('BEFORE DRAWNOW\n');print_tree(f,0);
      drawnow();fprintf('AFTER DRAWNOW\n');print_tree(f,0);
      handles=__go_handles__(true);root_before=get(0);properties=arrayfun(@(x) get(x),handles,'uniformoutput',false);
      d=__mf_figure_data__(f);fprintf('SERIALIZER %s\n',jsonencode(d));
      same=isequal(handles,__go_handles__(true))&&isequaln(root_before,get(0));
      for j=1:numel(handles),same=same&&isequaln(properties{j},get(handles(j)));endfor
      fprintf('EXTRACTION_PROPERTY_INVARIANCE %d\n',same);
    catch err
      fprintf('PROBE_ERROR %s\n',err.message);
    end_try_catch
  endfor
  close all;
endfunction
function print_tree(h,depth)
  fields={'type','visible','children','vertices','faces','facevertexcdata','xdata','ydata','zdata',...
    'fill','levellist','zlevel','zlevelmode','position','rotation','string','fontsize','fontunits','color',...
    'facecolor','edgecolor','cdatamapping','facealpha','edgealpha','alphadata','facevertexalphadata',...
    'alphadatamapping','alim','alimode','alphamap','facenormals','vertexnormals','facenormalsmode',...
    'vertexnormalsmode','facelighting','edgelighting','backfacelighting','ambientstrength','diffusestrength',...
    'specularstrength','specularexponent','specularcolorreflectance','style'};
  fprintf('OBJECT depth=%d handle=%.17g type=%s\n',depth,h,get(h,'type'));
  for name=fields
    if isprop(h,name{1})
      value=get(h,name{1});fprintf('PROPERTY %s class=%s shape=%s\n',name{1},class(value),mat2str(size(value)));disp(value);
    endif
  endfor
  % Native children, last-to-first draw order; hidden root inventory separately
  % belongs to the existing serializer tests. No root showhiddenhandles writes.
  if isprop(h,'children'),for child=flipud(get(h,'children')(:)).',print_tree(child,depth+1);endfor;endif
endfunction
