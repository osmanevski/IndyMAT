function verify_dalga26_forms(output_dir, only_case)
  % UNVERIFIED Qt acceptance recipe; run only in an orchestrator fresh copy.
  % No graphics/browser/MATLAB execution occurred while authoring this file.
  % only_case isolates each probe from the execute path's 12-figure cap.
  % Native-child mutations below test guards, not naturally generated holes.
  % Ledger: contourf native finite bands/order/limits, rotated 2D labels,
  % 3D label guards, vertex face/edge RGB + indexed mapping/classes, alpha,
  % lighting and budget PNG. Natural-hole inspection is diagnostic only.
  % MEASURE cases print observations only; they do not assert support or schema.
  if nargin<1,output_dir='';end
  if nargin<2,only_case='';end
  ramp='[~,h]=contourf([0 1 2],[0 1],[0 1 2;0 1 2],[0 1 2],"linecolor","none");colormap([1 0 0;0 1 0;0 0 1]);caxis([0 2]);';
  patchbase='h=patch("vertices",v,"faces",[1 2 3],"facevertexcdata",raw,"facecolor","interp","edgecolor","interp","cdatamapping",mapping);colormap([1 0 0;0 1 0;0 0 1]);caxis([0 2]);view(2);axis([0 1 0 1 0 2]);axis off;';
  cases={...
    'contourf finite bands','',ramp;...
    'contourf axis off','',[ramp 'axis([0 2 0 1]);axis off;'];...
    'contour labels group','', '[c,h]=contour(x,y,x+y,[-1 0 1]);clabel(c,h);';...
    'contour labels matrix','', 'c=contour(x,y,x+y,[-1 0 1]);clabel(c);';...
    'contourf labels PNG','contour_labels',[ramp 'clabel(get(h,"contourmatrix"),h);'];...
    'contour3 labels PNG','contour_labels','[c,h]=contour3(x,y,x+y,[-1 0 1]);clabel(c,h);';...
    'contourf background hole native','NATIVE_HOLE',[ramp 'ch=findobj(h,"type","patch");set(ch(1),"facevertexcdata",NaN,"facecolor",[1 1 1]);'];...
    'contourf compound PNG','contour_filled',[ramp 'ch=findobj(h,"type","patch");set(ch(1),"vertices",[0 0;1 0;1 1;0 1;2 0;3 0;3 1;2 1],"faces",[1 2 3 4;5 6 7 8]);'];...
    'contourf nonfinite PNG','contour_filled',[ramp 'ch=findobj(h,"type","patch");vv=get(ch(1),"vertices");vv(1,1)=NaN;set(ch(1),"vertices",vv);'];...
    'contourf total vertex budget PNG','budget_exceeded',[ramp 'ch=findobj(h,"type","patch");q=(0:100000)''*2*pi/100001;set(ch(1),"vertices",[cos(q),sin(q)],"faces",1:100001);'];...
    'patch indexed scaled','',['raw=[0;1;2];mapping="scaled";' patchbase];...
    'patch indexed direct double','',['raw=[1;2;3];mapping="direct";' patchbase];...
    'patch indexed direct uint8','',['raw=uint8([0;1;2]);mapping="direct";' patchbase];...
    'patch RGB double','',['raw=eye(3);mapping="scaled";' patchbase];...
    'patch RGB single','',['raw=single(eye(3));mapping="scaled";' patchbase];...
    'patch RGB uint8','',['raw=uint8(255*eye(3));mapping="direct";' patchbase];...
    'patch RGB uint16','',['raw=uint16(65535*eye(3));mapping="direct";' patchbase];...
    'patch edge only interp','',['raw=eye(3);mapping="scaled";' patchbase 'set(h,"facecolor","none");'];...
    'patch scalar alpha','ALPHA','h=patch("vertices",[0 0 1;1 0 1;1 1 1;0 1 1],"faces",1:4,"facecolor",[.2 .6 .8],"facealpha",.5);view(2);';...
    'surface scalar alpha','ALPHA','h=surf([0 1],[0 1],ones(2),"facealpha",.5,"edgealpha",.25);';...
    'surface edge indexed interp','', 'h=surf([0 1],[0 1],ones(2),[0 2;1 0],"facecolor","none","edgecolor","interp","linewidth",3);colormap([1 0 0;0 1 0;0 0 1]);caxis([0 2]);view(2);';...
    'surface edge RGB uint8 interp','', 'raw=uint8(255*cat(3,[1 0;0 1],[0 1;0 0],[0 0;1 0]));h=surf([0 1],[0 1],ones(2),raw,"facecolor","interp","edgecolor","interp","linewidth",3);view(2);';...
    'patch per vertex flat PNG','patch_colors','h=patch("vertices",v,"faces",[1 2 3],"facevertexcdata",[0;1;2],"facecolor","flat");view(3);';...
    'patch quad interp PNG','interpolated_color','patch("vertices",[0 0 1;1 0 1;1 1 1;0 1 1],"faces",1:4,"facevertexcdata",[0;1;0;1],"facecolor","interp");view(3);';...
    'patch pentagon interp PNG','interpolated_color','patch("vertices",[0 0 1;1 0 1;.5 .4 1;1 1 1;0 1 1],"faces",1:5,"facevertexcdata",[0;1;.5;1;0],"facecolor","interp");view(3);';...
    'patch wrong interp count PNG','patch_colors','h=patch("vertices",v,"faces",[1 2 3],"facevertexcdata",[0;1],"facecolor","interp");view(3);';...
    'surface scalar edge alpha','EDGE_ALPHA','h=surf([0 1],[0 1],ones(2),"edgealpha",.5);';...
    'patch scalar edge alpha','EDGE_ALPHA','h=patch("vertices",v,"faces",[1 2 3],"facecolor",[.2 .6 .8],"edgealpha",.5);view(3);';...
    'scatter marker alpha PNG','transparency','scatter3([0 1],[0 1],[1 1],40,"markerfacealpha",.5);';...
    'effective light PNG','lighting','surf(ones(2),"facelighting","flat");light;';...
    'surface vertex budget PNG','budget_exceeded','surf(zeros(201,200));';...
    'surface vertex alpha PNG','transparency','surf(ones(2),"facealpha","interp","alphadata",[0 .25;.5 1],"alphadatamapping","none");';...
    'patch face alpha PNG','transparency','patch("vertices",[v;1 1 1],"faces",[1 2 3;2 4 3],"facecolor",[.2 .6 .8],"facealpha","flat","facevertexalphadata",[.25;.75]);view(3);';...
    'patch flat light PNG','lighting','patch("vertices",v,"faces",[1 2 3],"facecolor",[.2 .6 .8],"facelighting","flat");light;view(3);';...
    'surface gouraud light PNG','lighting','surf(ones(2),"facelighting","gouraud");light;material shiny;';...
    'patch nonplanar PNG','unsupported_patch','patch("vertices",[0 0 1;1 0 1;1 1 2;0 1 1],"faces",[1 2 3 4],"facecolor",[.2 .6 .8]);view(3);';...
    'patch self crossing PNG','unsupported_patch','patch("vertices",[0 0 1;1 0 1;1 1 1;0 1 1],"faces",[1 3 2 4],"facecolor",[.2 .6 .8]);view(3);';...
    'patch vertex budget PNG','budget_exceeded','patch("vertices",[v;zeros(39998,2),ones(39998,1)],"faces",[1 2 3],"facecolor",[.2 .6 .8]);view(3);';...
    '2D interpolated patch PNG','interpolated_color','patch("vertices",[0 0;1 0;0 1],"faces",[1 2 3],"facevertexcdata",[0;1;2],"facecolor","interp");view(2);';...
    'natural hole inspection','INSPECT','[~,h]=contourf(x,y,x.^2+y.^2,[.5 1 2]);';...
    'contourf peaks20 default','SERIALIZE','[~,h]=contourf(peaks(20));';...
    'contourf peaks20 explicit5','SERIALIZE','[~,h]=contourf(peaks(20),[-4 -2 0 2 4]);';...
    'contourf peaks50 default','SERIALIZE','[~,h]=contourf(peaks(50));';...
    'contourf peaks100 default','SERIALIZE','[~,h]=contourf(peaks(100));'...
  };
  matched=false;
  for i=1:rows(cases)
    if ~isempty(only_case)&&~strcmp(only_case,cases{i,1}),continue;end
    matched=true;[x,y]=meshgrid(linspace(-2,2,9));v=[0 0 1;1 0 1;0 1 1];raw=[];mapping='';h=[];
    f=figure('name',['D26 ' cases{i,1}],'position',[0 0 400 400]);
    eval(cases{i,3});title(['D26 ' cases{i,1}]);
    handles=sort(findall(f));before=cell(size(handles));
    for j=1:numel(handles),before{j}=get(handles(j));end
    current=get(0,'currentfigure');
    started=tic();
    data=__mf_figure_data__(f,struct('job',repmat('f',1,32),'figure',i));
    serialize_seconds=toc(started);
    encoded=jsonencode(data);
    assert(isequal(handles,sort(findall(f))));assert(isequaln(current,get(0,'currentfigure')));
    for j=1:numel(handles),assert(isequaln(before{j},get(handles(j))),['Exporter mutated native object: ' cases{i,1}]);end
    assert(strcmp(data.source.job,repmat('f',1,32))&&data.source.figure==i);
    if strcmp(cases{i,2},'ALPHA')
      assert(data.supported,['Expected scalar alpha positive: ' cases{i,1}]);
      alpha_series=data.axes{1}.series{1};
      assert(isequal(alpha_series.face_alpha,struct('mode','constant','association','constant','value',.5)));
      if strcmp(cases{i,1},'surface scalar alpha')
        assert(isequal(alpha_series.edge_alpha,struct('mode','constant','association','constant','value',.25)));
      else
        assert(~isfield(alpha_series,'edge_alpha'),'Opaque default edge alpha is omitted');
      endif
    elseif strcmp(cases{i,2},'EDGE_ALPHA')
      assert(data.supported,['Expected scalar edge alpha positive: ' cases{i,1}]);
      alpha_series=data.axes{1}.series{1};
      assert(isequal(alpha_series.edge_alpha,struct('mode','constant','association','constant','value',.5)));
      assert(~isfield(alpha_series,'face_alpha'),'Opaque default face alpha is omitted');
    elseif strcmp(cases{i,2},'NATIVE_HOLE')
      assert(data.supported,'Constant-color NaN contour background must remain native');
      d26_assert_fields(data,'contourf background hole native',h,[], '');
    elseif strcmp(cases{i,2},'SERIALIZE')
      fprintf('D26 CONTOURF SERIALIZATION: %s; seconds=%.6f; json_bytes=%d; supported=%d; reason=%s; vertices=%d; triangles=%d\n',...
        cases{i,1},serialize_seconds,numel(unicode2native(encoded,'UTF-8')),data.supported,data.reason_code,data.vertex_count,data.triangle_count);
      assert(data.supported,['Expected contourf positive: ' cases{i,1}]);
      assert(data.version==3&&~isempty(data.axes{1}.series));
      assert(all(cellfun(@(q)strcmp(q.role,'contourf')&&strcmp(q.triangulation,'native-ring-v1'),data.axes{1}.series)));
    elseif strcmp(cases{i,2},'INSPECT')
      native=findobj(h,'type','patch');levels=NaN(1,numel(native));
      for k=1:numel(native),levels(k)=get(native(k),'facevertexcdata');end
      fprintf('D26 NATURAL HOLE INSPECTION: native NaN levels=%d; supported=%d; reason=%s\n',sum(isnan(levels)),data.supported,data.reason_code);
      disp(data.reason_args); % Diagnostic only: no unmeasured acceptance claim.
    elseif isempty(cases{i,2})
      assert(data.supported,['Expected interactive: ' cases{i,1}]);
      d26_assert_fields(data,cases{i,1},h,raw,mapping);
    else
      assert(~data.supported&&strcmp(data.reason_code,cases{i,2}),['Expected ' cases{i,2} ': ' cases{i,1}]);
      if any(strcmp(cases{i,1},{'patch quad interp PNG','patch pentagon interp PNG'})),assert(strcmp(data.reason_args.detail,'polygon_tessellation'));end
      if strcmp(cases{i,2},'contour_labels'),assert(strcmp(data.reason_args.property,'3d_text_depth'));end
      if strcmp(cases{i,1},'contourf compound PNG'),assert(strcmp(data.reason_args.detail,'compound_faces'));end
      if strcmp(cases{i,1},'contourf nonfinite PNG'),assert(strcmp(data.reason_args.detail,'holes_or_nonfinite'));end
      if strcmp(cases{i,2},'budget_exceeded'),assert(data.reason_args.actual>data.reason_args.limit);end
    end
    if ~isempty(output_dir)
      if ~exist(output_dir,'dir'),mkdir(output_dir);end
      file=fopen(fullfile(output_dir,sprintf('dalga26_%02d.json',i)),'w');assert(file>=0);
      fputs(file,jsonencode(data));fclose(file);
    end
    if ~any(strcmp(cases{i,2},{'INSPECT','MEASURE','ALPHA','EDGE_ALPHA'})),fprintf('D26 FORM PASS (real execution): %s\n',cases{i,1});end
    if isempty(only_case),close(f);end % Bound the batch independently of export caps.
  end
  assert(matched,['Unknown D26 case: ' only_case]);
end

function d26_assert_fields(data,name,h,raw,mapping)
  a=data.axes{1};series=a.series;
  if strncmp(name,'contourf ',9)
    assert(data.version==3&&a.dimension==3);assert(~isempty(series));
    native=flipud(get(h,'children')(:));
    native=native(arrayfun(@(p) strcmp(get(p,'type'),'patch')&&strcmp(get(p,'visible'),'on'),native));
    assert(numel(native)==numel(series));
    for k=1:numel(series)
      s=series{k};assert(strcmp(s.kind,'patch3d')&&strcmp(s.role,'contourf'));
      assert(s.contour_group==0&&s.index_base==0);
      assert(strcmp(s.triangulation,'native-ring-v1'));
      vv=reshape(cell2mat(s.vertices.values),cell2mat(s.vertices.shape));
      ff=reshape(cell2mat(s.source_faces.values),cell2mat(s.source_faces.shape))+1;
      canonical=reshape(cell2mat(s.faces.values),cell2mat(s.faces.shape))+1;
      p=native(k);nv=get(p,'vertices');if columns(nv)==2,nv=[nv zeros(rows(nv),1)];end
      native_faces=get(p,'faces');
      assert(isequaln(nv,vv)&&isequaln(native_faces,ff),...
        'Exported contour source geometry/order must match the native child');
      canonical_vertices=reshape(cell2mat(s.vertices.values),cell2mat(s.vertices.shape));
      canonical_vertices=canonical_vertices(canonical(1,:),:);
      expected_no_area=rows(unique(canonical_vertices,'rows'))<3;
      assert(s.no_area==expected_no_area);
      expected_triangles=max(0,columns(canonical)-2);if expected_no_area,expected_triangles=0;end
      assert(s.triangle_count==expected_triangles);
      if isnan(get(p,'facevertexcdata'))
        assert(isnan(s.level)&&s.background_hole&&strcmp(s.face_color.mode,'constant'));
      else
        assert(isequal(get(p,'facevertexcdata'),s.level)&&~s.background_hole);
      end
      assert(~isfield(s,'triangle_indices')&&~isfield(s,'triangle_faces')&&~isfield(s,'edge_indices'));
    end
  elseif strncmp(name,'contour labels ',15)
    assert(a.dimension==2);texts=series(cellfun(@(s)strcmp(s.kind,'text'),series));assert(~isempty(texts));
    native=findobj(h,'type','text');if isempty(native),native=findobj(gcf,'type','text');end
    for k=1:numel(texts)
      s=texts{k};assert(strcmp(s.units,'data')&&isfinite(s.rotation));
      found=false;
      for p=native(:).'
        r=mod(get(p,'rotation'),360);pos=get(p,'position');
        if isequaln(cell2mat(s.position),pos(1:2))&&s.rotation==r,found=true;break;end
      end
      assert(found,'Label position/rotation must preserve a native text object');
    end
  elseif strncmp(name,'patch ',6)
    assert(data.version==3&&a.dimension==3);s=series{1};
    assert(strcmp(s.kind,'patch3d')&&strcmp(s.role,'patch')&&s.index_base==0);
    assert(isequal(reshape(cell2mat(s.vertices.values),cell2mat(s.vertices.shape)),get(h,'vertices')));
    assert(isequal(reshape(cell2mat(s.faces.values),cell2mat(s.faces.shape))+1,get(h,'faces')));
    for field={'face_color','edge_color'}
      c=s.(field{1});if strcmp(c.mode,'none'),continue;end
      assert(strcmp(c.mode,'interp')&&strcmp(c.association,'vertex')&&strcmp(c.mapping,mapping));
      assert(strcmp(c.cdata_class,class(get(h,'facevertexcdata'))));
      assert(isequal(cell2mat(c.data.values),double(get(h,'facevertexcdata')(:).')));
      if columns(raw)==3,assert(strcmp(c.encoding,'truecolor')&&isequal(cell2mat(c.data.shape),[3 1 3]));
      else assert(strcmp(c.encoding,'indexed')&&isequal(cell2mat(c.data.shape),[3 1]));end
    end
  elseif strncmp(name,'surface edge ',13)
    s=series{1};assert(strcmp(s.kind,'surface')&&strcmp(s.edge_color.mode,'interp'));
    c=s.edge_color;assert(strcmp(c.association,'vertex')&&strcmp(c.cdata_class,class(get(h,'cdata'))));
    assert(isequal(cell2mat(c.data.values),double(get(h,'cdata')(:).')));
    assert(isequal(cell2mat(c.data.shape),size(get(h,'cdata'))));
  end
end
