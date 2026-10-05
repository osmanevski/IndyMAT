function __mf_debug_snapshot__(folder)
  mlock();
  result=struct('variables',{{}},'stack',{{}});
  try
    items=evalin('caller','(@whos)()');vars={};
    for k=1:min(numel(items),500)
      w=items(k);
      if strncmp(w.name,'__mf_',5),continue;endif
      s=struct('name',w.name,'size',sprintf('%dx',w.size),'class',w.class,'bytes',w.bytes,'complex',w.complex,'preview','');s.size=s.size(1:end-1);
      try
        if numel(w.size)<=2 && prod(w.size)<=12 && any(strcmp(w.class,{'double','single','logical','int8','int16','int32','int64','uint8','uint16','uint32','uint64','char'}))
          v=evalin('caller',w.name);if ischar(v),s.preview=v(1:min(numel(v),100));else s.preview=mat2str(v,5);endif
        elseif strcmp(w.class,'function_handle'),s.preview=func2str(evalin('caller',w.name));
        else s.preview=sprintf('%s %s',s.size,w.class);
        endif
      catch,s.preview=sprintf('%s %s',s.size,w.class);end_try_catch
      vars{end+1}=s;
    endfor
    result.variables=vars;
  catch
  end_try_catch
  try
    [raw_stack,raw_index]=dbstack();frames={};current=0;has_current=false;
    for k=1:numel(raw_stack)
      if strncmp(raw_stack(k).name,'__mf_',5),continue;endif
      current=current+1;
      frame=struct('index',current,'name',raw_stack(k).name,'file',raw_stack(k).file,'line',raw_stack(k).line,'column',raw_stack(k).column,'current',k==raw_index);
      if frame.current,has_current=true;endif
      frames{end+1}=frame;
    endfor
    if !has_current && !isempty(frames),frames{1}.current=true;endif
    result.stack=frames;
  catch
  end_try_catch
  fid=fopen(fullfile(folder,'debug.json'),'w');if fid>=0,fputs(fid,jsonencode(result));fclose(fid);endif
  [~,job]=fileparts(folder);fprintf('\n__MF_DEBUG_%s__\n',job);fflush(stdout);
endfunction
