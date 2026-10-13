function verify_dalga24_interp(output_dir)
  % Manual Qt/browser/MATLAB acceptance, in a fresh orchestrator test copy only.
  % Optional output_dir writes REAL serializer artifacts for Octave inspection.
  % No graphics were run in the implementation lane. No synthetic fixtures.
  % Viewer diagonal: (r+1,c)--(r,c+1); triangles (0,2,1),(1,2,3).
  % Viewer interpolation: map each vertex to sRGB, then interpolate RGB.
  % Data tips: nearest original cell vertex XYZ + raw scalar/RGB CData.
  if nargin<1,output_dir='';end
  d24_figures={};
  d24_figures{end+1}=figure('name','D24 peaks 80 interp turbo');
  [d24_x,d24_y]=meshgrid(linspace(-3,3,80));
  surf(d24_x,d24_y,peaks(d24_x,d24_y));shading interp;colormap(turbo(256));colorbar;
  title('D24 peaks 80x80: interp, turbo, colorbar');xlabel('x');ylabel('y');zlabel('z');view(-35,30);

  d24_map=[1 0 0;0 1 0;0 0 1;1 1 0];
  d24_figures{end+1}=figure('name','D24 2x2 diagonal RGB');
  surf([0 1],[0 1],zeros(2),[0 2;1 3],'facecolor','interp','edgecolor','none');
  colormap(d24_map);caxis([0 4]);colorbar;view(0,90);
  title('D24 2x2: fixed off-origin diagonal; RGB interpolation');

  d24_figures{end+1}=figure('name','D24 3x3 diagonal RGB');
  surf(0:2,0:2,[0 .1 0;.1 0 .1;0 .1 0],[0 3 2;2 1 0;3 0 1],...
       'facecolor','interp','edgecolor','none');
  colormap(d24_map);caxis([0 4]);colorbar;view(0,90);
  title('D24 3x3: corner values expose diagonal and interpolation space');

  d24_figures{end+1}=figure('name','D24 truecolor interp');
  d24_rgb=uint8(255.*cat(3,[1 0;0 1],[0 1;0 1],[0 0;1 0]));
  surf([0 1],[0 1],zeros(2),d24_rgb,'facecolor','interp','edgecolor','none');view(0,90);
  title('D24 uint8 truecolor interp: tips report raw RGB bytes');

  d24_figures{end+1}=figure('name','D24 interp NaN hole');
  d24_z=peaks(5);d24_z(3,3)=NaN;
  surf(d24_z);shading interp;colormap(turbo(256));colorbar;
  title('D24 interp: NaN XYZ/CData removes all incident cells');

  d24_figures{end+1}=figure('name','D24 reversed direct interp');
  surf(0:2,0:2,zeros(3),uint8([0 3 1;2 0 3;3 1 2]),...
       'facecolor','interp','edgecolor','none','cdatamapping','direct');
  colormap(flipud(d24_map));colorbar;view(0,90);
  title('D24 reversed colormap: integer direct indexing, interp');

  d24_figures{end+1}=figure('name','D24 interp edges (admitted in D26)');
  surf([0 1;1 0],'facecolor','interp','edgecolor','interp');
  title('D26 edge interp: both endpoint colours');

  d24_figures{end+1}=figure('name','D24 scalar alpha interp (admitted in D26)');
  surf([0 1;1 0],'facecolor','interp','edgecolor','interp','facealpha',.5);
  title('D26 scalar alpha: interactive interp surface');

  d24_figures{end+1}=figure('name','D24 data-valued alpha PNG fallback');
  surf([0 1;1 0],'facecolor','interp','edgecolor','interp',...
       'facealpha','interp','alphadata',[0 .25;.5 1],'alphadatamapping','none');
  title('Data-valued alpha remains PNG: transparency');

  if ~isempty(output_dir)
    if ~exist(output_dir,'dir'),mkdir(output_dir);end
    for d24_i=1:numel(d24_figures)
      d24_handles=sort(findall(d24_figures{d24_i}));d24_before=cell(size(d24_handles));
      for d24_j=1:numel(d24_handles),d24_before{d24_j}=get(d24_handles(d24_j));end
      d24_current=get(0,'currentfigure');
      d24_data=__mf_figure_data__(d24_figures{d24_i},struct('job',repmat('f',1,32),'figure',d24_i));
      assert(isequal(d24_handles,sort(findall(d24_figures{d24_i}))));
      assert(isequaln(d24_current,get(0,'currentfigure')));
      for d24_j=1:numel(d24_handles),assert(isequaln(d24_before{d24_j},get(d24_handles(d24_j))));end
      if d24_i<numel(d24_figures)
        assert(d24_data.supported);
        assert(strcmp(d24_data.axes{1}.series{1}.face_color.mode,'interp'));
        assert(strcmp(d24_data.axes{1}.series{1}.face_color.association,'vertex'));
        if d24_i>=7
          assert(strcmp(d24_data.axes{1}.series{1}.edge_color.mode,'interp'));
          assert(strcmp(d24_data.axes{1}.series{1}.edge_color.association,'vertex'));
        end
        if d24_i==8
          assert(isequal(d24_data.axes{1}.series{1}.face_alpha,struct('mode','constant','association','constant','value',.5)));
        end
      else
        assert(~d24_data.supported&&strcmp(d24_data.reason_code,'transparency'));
        assert(strcmp(d24_data.reason_args.detail,'unmeasured_data_mapping'));
      end
      d24_file=fopen(fullfile(output_dir,sprintf('interp_%02d.json',d24_i)),'w');
      assert(d24_file>=0);fputs(d24_file,jsonencode(d24_data));fclose(d24_file);
    end
  end
end
