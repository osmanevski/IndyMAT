function uret(output_dir = fileparts(mfilename('fullpath')))
  % REAL graphics only. No mocks and no hand-authored JSON fixture substitutes.
  % Invoke in a fresh Octave process; this generator owns and closes its figures.
  root=fileparts(fileparts(fileparts(fileparts(mfilename('fullpath')))));
  addpath(fullfile(root,'octave'));
  if ~exist(output_dir,'dir'),mkdir(output_dir);endif
  names={'line2d','plot3_gap','scatter3_sizes','surf_vector','surf_matrix_nan',...
    'mesh_default','surf_integer_clim','colorbar','reversed_view',...
    'no_axes','budget_exceeded','invalid_data','unsupported_object','unsupported_group',...
    'unsupported_marker','unsupported_color','scatter_colors','transparency','lighting',...
    'interpolated_color','unsupported_surface','log_3d','perspective','manual_camera',...
    'unsupported_units','unsupported_colorbar','json_budget',...
    'bar_grouped','barh_stacked','hist_flat','area_stacked','patch_missing_vertex',...
    'text_boxes','group_lines','figure_title',...
    'patch_colors','unsupported_patch','unsupported_text'};
  supported=[names(1:9),{'bar_grouped','barh_stacked','hist_flat','area_stacked',...
    'patch_missing_vertex','text_boxes','group_lines','figure_title'}];
  for k=1:numel(names)
    close all;
    f=figure(1,'visible','off');name=names{k};
    switch name
      case 'line2d'
        plot([1 2 4],[3 5 4],'-o','markerfacecolor','auto');
      case 'plot3_gap'
        plot3([1 2 NaN 4 5],[2 3 NaN 6 7],[3 4 NaN 8 9],'-o','color',[.2 .4 .6]);
      case 'scatter3_sizes'
        scatter3([1 2 3],[4 5 6],[7 8 9],[9 25 49],[.2 .4 .6],'filled');
      case 'surf_vector'
        surf([10 20 30],[40 50],[1 3 5;2 4 6]);
      case 'surf_matrix_nan'
        surf([10 20 30;11 21 31],[40 41 42;50 51 52],[1 3 NaN;2 4 6]);
      case 'mesh_default'
        mesh([10 20 30],[40 50],[1 3 5;2 4 6]);
      case 'surf_integer_clim'
        surf([10 20 30],[40 50],[1 3 5;2 4 6],uint8([0 2 4;1 3 5]));
        set(gca,'clim',[-1 8]);colormap(gca,[0 0 0;1 0 0;0 1 0;0 0 1]);
      case 'colorbar'
        surf([1 3 5;2 4 6]);cb=colorbar();ylabel(cb,'Intensity');
      case 'reversed_view'
        plot3([1 2 3],[4 5 6],[7 8 9]);view(25,35);
        set(gca,'xdir','reverse','ydir','reverse','zdir','reverse','dataaspectratio',[1 2 3]);
      case 'no_axes'
      case 'budget_exceeded'
        surf(zeros(201,200));
      case 'invalid_data'
        h=plot3(1:3,4:6,7:9);set(h,'ydata',[1 2]);
      case 'unsupported_object'
        patch('vertices',[0 0 0;1 0 0;0 1 0],'faces',[1 2 3],'facecolor',[1 0 0]);view(3);
      case 'unsupported_group'
        ax=axes();h=hggroup('parent',ax);line('parent',h,'xdata',[1 2],'ydata',[2 3]);
      case 'unsupported_marker'
        plot3(1:3,4:6,7:9,'-d');
      case 'unsupported_color'
        scatter(1:3,4:6,36,1);
      case 'scatter_colors'
        scatter3(1:4,5:8,9:12,36,[1 0 0;0 1 0;0 0 1;1 1 0]);
      case 'transparency'
        surf([1 3 5;2 4 6],'facealpha',.5);
      case 'lighting'
        surf([1 3 5;2 4 6],'facelighting','flat');light();
      case 'interpolated_color'
        surf([1 3 5;2 4 6],'facecolor','interp');
      case 'unsupported_surface'
        surf([1 3 5;2 4 6],'facecolor','texturemap');
      case 'log_3d'
        plot3(1:3,4:6,7:9);set(gca,'xscale','log');
      case 'perspective'
        plot3(1:3,4:6,7:9);set(gca,'projection','perspective');
      case 'manual_camera'
        plot3(1:3,4:6,7:9);set(gca,'cameraposition',[10 20 30]);
      case 'unsupported_units'
        panel=uipanel('parent',f);ax=axes('parent',panel);plot3(ax,1:3,4:6,7:9);
      case 'unsupported_colorbar'
        plot3(1:3,4:6,7:9);cb=colorbar();set(cb,'__axes_handle__',[]);
      case 'bar_grouped'
        bar([10 20 30],[1 2;3 4;2 5],0.6,'basevalue',0.5);legend('a','b');
      case 'barh_stacked'
        barh([1 2;3 4],'stacked');
      case 'hist_flat'
        hist([1 2 2 3 3 3],3);
      case 'area_stacked'
        area(1:3,[1 2;2 1;3 3]);
      case 'patch_missing_vertex'
        fill([0 1 1],[0 0 1],'r','linestyle','--');hold on;
        patch([2 3 3 2;4 5 4.5 NaN]',[0 0 1 1;0 0 1 NaN]',[0 .6 0],'edgecolor','none');
      case 'text_boxes'
        plot(1:3);
        text(1.5,2.5,{'two lines','\sigma_x^2'},'backgroundcolor','w','edgecolor','k','verticalalignment','top');
        text(2,1.5,['ab ';'cde'],'rotation',90,'fontweight','bold','fontangle','italic','color',[0 .5 0],'fontsize',14);
        text(.5,.9,'top','units','normalized','horizontalalignment','center','interpreter','none','margin',6);
      case 'group_lines'
        errorbar(1:3,[1 2 3],[.1 .2 .1]);hold on;xline(2,'--','L');yline(2.5,'r');
      case 'figure_title'
        plot(1:3);axes('position',[0 .94 1 .06],'visible','off');
        text(.5,.5,'Figure title','horizontalalignment','center','fontweight','bold','fontsize',12);
      case 'patch_colors'
        patch([0 1 1;2 3 3]',[0 0 1;0 0 1]',[1;2]);
      case 'unsupported_patch'
        fill([0 1 1],[0 0 1],'r','marker','o');
      case 'unsupported_text'
        plot(1:3);text(40,40,'pixels','units','pixels');
      case 'json_budget'
        % The cap is deliberately tested through the real export entry point,
        % never by constructing a fallback record in this generator.
        plot3(1:3,4:6,7:9,'displayname',repmat('x',1,8388608));
    endswitch
    if strcmp(name,'json_budget')
      parent=tempname();mkdir(parent);folder=fullfile(parent,repmat('f',1,32));mkdir(folder);
      unwind_protect
        fid=fopen(fullfile(folder,'code.m'),'w');fclose(fid);
        __mf_execute__(folder,'code','');
        bytes=fileread(fullfile(folder,'figure-1.json'));data=jsondecode(bytes);
        assert(~data.supported&&strcmp(data.reason_code,'json_budget'));
      unwind_protect_cleanup
        rmdir(parent,'s');
      end_unwind_protect
    else
      % Compare ALL raw graphics properties, not just the exported subset.
      handles=sort(findall(f));before=cell(numel(handles),1);
      for j=1:numel(handles),before{j}=get(handles(j));endfor
      root_before=get(0,'currentfigure');base_before=evalin('base','whos');
      data=__mf_figure_data__(f,struct('job',repmat('f',1,32),'figure',1));
      assert(isequaln(base_before,evalin('base','whos')));
      assert(isequaln(root_before,get(0,'currentfigure')));
      assert(isequal(handles,sort(findall(f))));
      for j=1:numel(handles),assert(isequaln(before{j},get(handles(j))));endfor
      bytes=jsonencode(data);
      if any(strcmp(name,supported)),assert(data.supported,name);
      else assert(~data.supported&&strcmp(data.reason_code,name),name);endif
    endif
    fid=fopen(fullfile(output_dir,[name '.json']),'w');assert(fid>=0);fputs(fid,bytes);fclose(fid);
    fprintf('%s: version=%d supported=%d reason=%s\n',name,data.version,data.supported,data.reason_code);
  endfor
  close all;
endfunction
