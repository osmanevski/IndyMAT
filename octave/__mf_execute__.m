function __mf_execute__(folder, mode, argument)
  % Protocol entry. The kernel calls it as (@__mf_execute__)(...): a function
  % handle literal resolves the function even when a base-workspace variable has
  % the same name, so nothing a user variable can shadow is needed at top level.
  % Everything below runs in this function's own workspace, where user variables
  % are not visible; only the user's code and base-name lookups use evalin.
  % Documented limit: user FUNCTION FILES that shadow built-ins (fprintf.m ...)
  % are not defended against.
  mlock();
  % Only builtins here and in the completion at the end: if the user emptied the load path
  % (path('')), fullfile/fileparts are gone, and the job must still end with result.json.
  preapply = [folder filesep() 'preapply.txt'];
  job = regexp(folder, '[^/\\]+$', 'match', 'once'); setappdata(0,'__mf_job__',job); setappdata(0,'__mf_folder__',folder);
  result = struct('error', '', 'variables', {{}}, 'detail', []);
  workspace_reader = @__mf_workspace_base__;
  if ~any(strcmp(mode,{'inspect','snapshot','package','workspace-rename','workspace-assign-scalar','workspace-clear-names','workspace-save','workspace-load-inspect','workspace-load','variable-read','variable-write'})),result.figures={};endif
  % A bare `return` typed by the user (base scope or paused prompt) unwinds this
  % function; the completion below runs in the cleanup so the job still ends.
  unwind_protect
    try
      if !strcmp(mode, 'snapshot') && exist(preapply, 'file') == 2
        try
          rehash();
          preapply_code = fileread(preapply);
        catch preapply_error
          error(__mf_text__('Could not apply breakpoints (could not read the preparation file): %s', 'Kesme noktaları uygulanamadı (hazırlık dosyası okunamadı): %s'), preapply_error.message);
        end_try_catch
        __mf_breakpoint_eval__(preapply_code);
      endif
      if strcmp(mode, 'breakpoint') && !isempty(argument)
        result.breakpoint_relocation = __mf_breakpoint_ops__(argument);
      elseif strcmp(mode, 'breakpoint')
        __mf_breakpoint_eval__(fileread(fullfile(folder, 'code.m')));
      elseif strcmp(mode, 'code')
        evalin('base', fileread(fullfile(folder, 'code.m')));
      elseif strcmp(mode, 'file')
        try,bp=dbstatus();catch,bp=[];end_try_catch
        debug_file=false;
        for b=1:numel(bp),if strcmp(bp(b).file,argument),debug_file=true;break;endif,endfor
        if debug_file
          [parent,name]=fileparts(argument);previous=pwd();changed=~strcmp(previous,parent);
          unwind_protect
            if changed,cd(parent);dbstop(bp);endif
            evalin('base',[name ';']);
          unwind_protect_cleanup
            if changed,cd(previous);endif
          end_unwind_protect
        else
          evalin('base', sprintf('(@run)(''%s'');', strrep(argument, '''', '''''')));
        endif
      elseif strcmp(mode, 'profile')
        profile clear; profile on;
        unwind_protect
          evalin('base', sprintf('(@run)(''%s'');', strrep(argument, '''', '''''')));
        unwind_protect_cleanup
          profile off;
        end_unwind_protect
        result.profile = __mf_profile_rows__(profile('info'), argument);
      elseif strcmp(mode, 'publish')
        if exist('publish','file') ~= 2,error(__mf_text__('The GNU Octave publish function was not found in this installation.', 'GNU Octave publish işlevi bu kurulumda bulunamadı.'));endif
        source_dir=fileparts(argument); [~,source_name]=fileparts(argument); output_dir=fullfile(source_dir,'html');
        try
          output_file=__mf_publish__(argument);
        catch publish_error
          error(__mf_text__('Could not create the HTML report: %s', 'HTML raporu oluşturulamadı: %s'),publish_error.message);
        end_try_catch
        images=dir(fullfile(folder,'publish','*.png'));
        % A bare Kernel has staged output only. App's completion adapter clears
        % this flag after Python has sanitised and installed the final report.
        result.publish=struct('path',output_file,'name',[source_name '.html'],'images',numel(images),'staged',true);
      elseif strcmp(mode, 'rotate')
        args=jsondecode(argument);
        if ~isgraphics(args.figure,'figure'),error(__mf_text__('The figure was closed', 'Grafik kapandı'));endif
        previous=get(0,'currentfigure');
        unwind_protect
          ax=get(args.figure,'currentaxes'); [az,el]=view(ax); view(ax,az+args.angle,el);
        unwind_protect_cleanup
          if ~isempty(previous)&&isgraphics(previous,'figure'),set(0,'currentfigure',previous);endif
        end_unwind_protect
      elseif strcmp(mode, 'inspect')
        if isempty(regexp(argument, '^[A-Za-z][A-Za-z0-9_]*$', 'once'))
          error(__mf_text__('Invalid variable name', 'Geçersiz değişken adı'));
        endif
        value = evalin('base', argument);
        result.detail = __mf_preview__(value, argument);
      elseif strcmp(mode, 'package')
        args=jsondecode(argument);
        if ~any(strcmp(args.action,{'load','unload'})) || isempty(regexp(args.name,'^[A-Za-z][A-Za-z0-9_]*$','once'))
          error(__mf_text__('Invalid package operation', 'Geçersiz paket işlemi'));
        endif
        pkg('prefix',args.prefix,args.archprefix); pkg('local_list',args.registry);
        pkg(args.action,args.name);
      elseif any(strcmp(mode, {'workspace-rename','workspace-assign-scalar','workspace-clear-names','workspace-save','workspace-load-inspect','workspace-load'}))
        result.workspace_action = __mf_workspace__(mode, argument);
      elseif any(strcmp(mode, {'variable-read','variable-write'}))
        result.variable_action = __mf_variable__(mode, argument);
      endif
    catch err
      result.error = err.message;
      result.error_frames = err.stack;
      for k = 1:min(numel(err.stack), 6)
        if isempty(strfind(err.stack(k).name, '__mf_'))
          result.error = sprintf('%s\n  %s:%d', result.error, err.stack(k).name, err.stack(k).line);
        endif
      endfor
    end_try_catch
  unwind_protect_cleanup
    try
      items = workspace_reader('whos');
      vars = {};
      for k = 1:min(numel(items), 500)
        w=items(k);
        s = struct('name', w.name, 'size', sprintf('%dx', w.size), 'class', w.class, 'bytes', w.bytes, 'complex', w.complex, 'preview', '');
        s.size=s.size(1:end-1);
        s.global=w.global;
        try
          if numel(w.size)<=2 && prod(w.size)<=12 && any(strcmp(w.class, {'double','single','logical','int8','int16','int32','int64','uint8','uint16','uint32','uint64','char'}))
            v=evalin('base',w.name);
            if ischar(v)
              % Never put a split/invalid UTF-8 byte into the shared job envelope.
              if isrow(v) || isempty(v)
                try, s.preview=native2unicode(uint8(v),'UTF-8'); catch, s.preview=__mf_text__('[Invalid UTF-8]', '[Geçersiz UTF-8]'); end_try_catch
              else
                s.preview=sprintf('%s char',s.size);
              endif
            else s.preview=mat2str(v,5); endif
          elseif strcmp(w.class,'function_handle')
            s.preview=func2str(evalin('base',w.name));
          else
            s.preview=sprintf('%s %s',s.size,w.class);
          endif
        catch
          s.preview=sprintf('%s %s',s.size,w.class);
        end_try_catch
        vars{end+1}=s;
      endfor
      result.variables=vars;
      if ~any(strcmp(mode,{'inspect','snapshot','package','workspace-rename','workspace-assign-scalar','workspace-clear-names','workspace-save','workspace-load-inspect','workspace-load','variable-read','variable-write'}))
      figs=findall(0,'type','figure');
      figs=sort(figs);
      for k=1:min(numel(figs),12)
        f=figs(k); name=get(f,'name');
        if isempty(name), name=sprintf('Figure %g',double(f)); endif
        file=sprintf('figure-%d.png',k);
        try
          datafile=sprintf('figure-%d.json',k); interactive=false; decimated=false; fallback_reason=__mf_text__('Could not generate interactive plot data.', 'Etkileşim verisi üretilemedi.'); datafid=-1;
          try
            figure_data=__mf_figure_data__(f); interactive=logical(figure_data.supported); decimated=logical(figure_data.decimated); fallback_reason=figure_data.reason;
            datafid=fopen(fullfile(folder,datafile),'w');
            if datafid<0,error(__mf_text__('Could not write interactive plot data', 'Etkileşim verisi yazılamadı'));endif
            fputs(datafid,jsonencode(figure_data)); fclose(datafid);
          catch dataerr
            if exist('datafid','var')&&datafid>=0,fclose(datafid);endif
            fallback_reason=sprintf(__mf_text__('Could not generate interactive plot data: %s', 'Etkileşim verisi üretilemedi: %s'),dataerr.message);
          end_try_catch
          set(f,'paperpositionmode','auto');
          print(f,fullfile(folder,file),'-dpng','-r120');
          result.figures{end+1}=struct('name',name,'file',file,'number',double(f),'data_file',datafile,'interactive',interactive,'decimated',decimated,'fallback_reason',fallback_reason);
        catch ploterr
          fprintf(2,__mf_text__('Could not export the figure: %s\n', 'Grafik dışa aktarılamadı: %s\n'),ploterr.message);
        end_try_catch
      endfor
      endif
      result.version=version();
      result.cwd=pwd();
      result.toolkits=available_graphics_toolkits();
      result.packages={};
      packages=pkg('list');
      for k=1:numel(packages)
        result.packages{end+1}=struct('name',packages{k}.name,'version',packages{k}.version,'loaded',packages{k}.loaded);
      endfor
    catch metaerr
      fprintf(2,__mf_text__('Could not read the workspace: %s\n', 'Çalışma alanı okunamadı: %s\n'),metaerr.message);
    end_try_catch
    fid=fopen([folder filesep() 'result.json'],'w');
    if fid>=0
      fputs(fid,jsonencode(result)); fclose(fid);
    endif
    % Private verification facts, never editor code or a browser-supplied path.
    % Keep this separate from result.json so native state payloads stay intact.
    try
      environment=struct('constructor',which('string'),'path',path(),'cwd',pwd());
      fid=fopen([folder filesep() 'source-environment.json'],'w');
      if fid>=0,fputs(fid,jsonencode(environment));fclose(fid);endif
    catch
    end_try_catch
    fprintf('\n__MF_DONE_%s__\n',job); fflush(stdout);
  end_unwind_protect
endfunction

% Breakpoint helpers are subfunctions of this file on purpose: relocation jobs are
% entered through a stored handle and skip the path commands, so everything they
% call must resolve lexically even after the user removed the application folder
% from the load path.
function __mf_breakpoint_eval__(__mf_code__)
  % Evaluates breakpoint command text in this private function workspace, never
  % in the user's base workspace, so user variables cannot shadow dbstop/addpath.
  eval(__mf_code__);
endfunction

function results = __mf_breakpoint_ops__(argument)
  % Runs breakpoint operations, one try block each, and reports the outcome per
  % operation: {id, ok, message}. An operation with a "requires" id is skipped
  % (reported as failed) when that earlier operation failed, so a new location is
  % never installed while the old one could not be cleared. Command text is
  % evaluated in a private function workspace (see __mf_breakpoint_eval__).
  results = {};
  request = jsondecode(argument);
  ops = request.ops;
  failed = {};
  for index = 1:numel(ops)
    if iscell(ops), item = ops{index}; else item = ops(index); endif
    outcome = struct('id', item.id, 'ok', true, 'message', '');
    try
      if isfield(item, 'requires') && any(strcmp(failed, item.requires))
        error(__mf_text__('The new breakpoint was not set because the previous breakpoint could not be cleared.', 'Önceki kesme noktası temizlenemediği için yeni konum kurulmadı.'));
      endif
      __mf_breakpoint_eval__(item.code);
    catch err
      outcome.ok = false;
      outcome.message = err.message;
      failed{end+1} = item.id;
    end_try_catch
    results{end+1} = outcome;
  endfor
endfunction

function __mf_breakpoint_clear_moved__(name, lines, resolver)
  % Clears breakpoints of a function whose file was renamed, moved or trashed.
  % After the move Octave refuses dbclear(name) because the old path no longer
  % resolves; a temporary autoload gives the stale function-table entry a real
  % resolver. The mapping is restored (or removed) even when dbclear fails, and
  % nothing is cleared from the function table, so command-line functions and
  % retained handles survive.
  if exist(name) == 103
    error(__mf_text__('"%s" is currently a command-line function; Octave cannot safely clear its breakpoints.', '"%s" şu anda bir komut satırı işlevi; Octave bunun kesme noktalarını güvenle temizleyemez.'), name);
  endif
  mappings = autoload();
  if isempty(mappings), original = []; else original = mappings(strcmp({mappings.function}, name)); endif
  autoload(name, resolver);
  unwind_protect
    for index = 1:numel(lines)
      __mf_dbclear__(name, sprintf('%d', lines(index)), true);
    endfor
  unwind_protect_cleanup
    autoload(name, resolver, 'remove');
    if !isempty(original), autoload(name, original(1).file); endif
  end_unwind_protect
endfunction

function __mf_dbclear__(name, line, strict = false)
  % dbclear that refuses a command-line function: Octave 11.3 crashes the whole
  % process (SIGSEGV, not catchable) when dbclear names one. exist() reports 103
  % for them and is safe to call. Any other dbclear failure is ignored, like the
  % former try/catch around each call, unless strict is true.
  if exist(name) == 103
    error(__mf_text__('"%s" is a command-line function; Octave does not support clearing its breakpoints.', '"%s" bir komut satırı işlevi; Octave bu işlevde kesme noktası temizlemeyi desteklemiyor.'), name);
  endif
  if strict
    dbclear(name, line);
  else
    try
      dbclear(name, line);
    catch
    end_try_catch
  endif
endfunction

function rows=__mf_profile_rows__(info,source)
  m=numel(info.FunctionTable); self_times=zeros(1,m); total_times=zeros(1,m); [self_times,total_times]=__mf_profile_times__(info.Hierarchical,self_times,total_times); rows={};
  for k=1:m
    item=info.FunctionTable(k); [file,line]=__mf_profile_location__(item.FunctionName,source);
    rows{end+1}=struct('name',item.FunctionName,'calls',item.NumCalls,'total',total_times(k),'self',self_times(k),'file',file,'line',line);
  endfor
endfunction

function [self_times,total_times]=__mf_profile_times__(tree,self_times,total_times)
  for k=1:numel(tree)
    node=tree(k); self_times(node.Index)=self_times(node.Index)+node.SelfTime; total_times(node.Index)=total_times(node.Index)+node.TotalTime;
    if ~isempty(node.Children),[self_times,total_times]=__mf_profile_times__(node.Children,self_times,total_times);endif
  endfor
endfunction

function [file,line]=__mf_profile_location__(name,source)
  file=''; line=0; [source_dir,source_name]=fileparts(source); simple=regexp(name,'[A-Za-z][A-Za-z0-9_]*$','match','once');
  if isempty(simple),return;endif
  candidate=fullfile(source_dir,[simple '.m']);
  if strcmp(simple,source_name),candidate=source;
  elseif exist(candidate,'file') ~= 2
    candidate=source; found=__mf_function_line__(candidate,simple);
    if found==0,candidate=which(simple);endif
  endif
  if isempty(candidate) || exist(candidate,'file') ~= 2,return;endif
  file=canonicalize_file_name(candidate); line=__mf_function_line__(file,simple);
  if line==0 && strcmp(simple,source_name),line=1;endif
endfunction

function line=__mf_function_line__(file,name)
  line=0;
  try
    lines=strsplit(fileread(file),'\n'); pattern=['^\s*function\s+.*\<' regexptranslate('escape',name) '\>\s*\('];
    for k=1:numel(lines),if ~isempty(regexp(lines{k},pattern,'once')),line=k;return;endif,endfor
  catch
  end_try_catch
endfunction

function result=__mf_preview__(v,name)
  mlock();
  result=struct('name',name,'class',class(v),'size',size(v),'rows',{{}},'text','','truncated',false);
  if isnumeric(v) || islogical(v)
    if ndims(v)<=2
      for r=1:min(rows(v),100)
        row={};
        for c=1:min(columns(v),30), row{end+1}=num2str(v(r,c),10); endfor
        result.rows{end+1}=row;
      endfor
      result.truncated=rows(v)>100 || columns(v)>30;
    else
      result.text=sprintf(__mf_text__('%s — multidimensional array; use indexing to inspect it.', '%s — çok boyutlu dizi; indeksleyerek inceleyin.'),mat2str(size(v)));
    endif
  elseif ischar(v)
    shown=v(1:min(rows(v),100),1:min(columns(v),80));
    result.text=strjoin(cellstr(shown),'\n'); result.truncated=rows(v)>100 || columns(v)>80;
  elseif iscell(v)
    result.text=sprintf('Cell array %s\n',mat2str(size(v)));
    for k=1:min(numel(v),30)
      result.text=[result.text sprintf('{%d}: %s %s\n',k,class(v{k}),mat2str(size(v{k})))];
    endfor
    result.truncated=numel(v)>30;
  elseif isstruct(v)
    result.text=sprintf(__mf_text__('Struct %s\nFields:\n%s', 'Struct %s\nAlanlar:\n%s'),mat2str(size(v)),strjoin(fieldnames(v),'\n'));
  elseif isa(v,'function_handle')
    result.text=func2str(v);
  else
    result.text=sprintf('%s %s',class(v),mat2str(size(v)));
  endif
endfunction

% Local selector: lexical lookup survives path removal and user functions
% named __mf_text__; caller/base variables cannot enter this workspace.
function text = __mf_text__(english, turkish)
  text = english;
  if strcmp(getenv('INDYMAT_LANGUAGE'), 'tr'), text = turkish; endif
endfunction
