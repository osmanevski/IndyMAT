function verify_dalga26_contour_budget()
  % UNVERIFIED real graphics probe. Prints exact native shapes and rejection
  % arguments; intended for the orchestrator, never the sandbox unit runner.
  recipes={'contourf(peaks(20))','contourf(peaks(20),[-4 -2 0 2 4])','contourf([1 2;3 4])','contourf([0 1 2],[0 1],[0 1 2;0 1 2],[0 1 2],"linecolor","none")'};
  for i=1:numel(recipes)
    f=figure('name',['D26 native contour budget ' num2str(i)],'visible','off');
    eval(['[~,h]=' recipes{i} ';']);title(recipes{i});drawnow;
    children=flipud(get(h,'children')(:));patches={};
    for child=children.'
      if ~strcmp(get(child,'type'),'patch'),continue;endif
      patches{end+1}=struct('vertices',get(child,'vertices'),'faces',get(child,'faces'),...
        'zdata',get(child,'zdata'),'facevertexcdata',get(child,'facevertexcdata'),...
        'facecolor',get(child,'facecolor'),'edgecolor',get(child,'edgecolor'));
    endfor
    data=__mf_figure_data__(f);
    fprintf('D26_CONTOUR %s\n',jsonencode(struct('recipe',recipes{i},'patches_in_draw_order',{patches},...
      'supported',data.supported,'reason_code',data.reason_code,'reason_args',data.reason_args,...
      'vertex_count',data.vertex_count,'triangle_count',data.triangle_count)));
    close(f);
  endfor
endfunction
