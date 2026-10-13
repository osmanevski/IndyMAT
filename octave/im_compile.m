function plan = im_compile(model)
  % IM_COMPILE Validate an IndyMAT block model (struct or JSON text).
  % Experimental documented subset; not Simulink compatibility.
  % See block-models.md. This function never evaluates model expressions.
  if ischar(model)
    if !isrow(model) || numel(model) > 4*1024*1024
      error('im:model:JSON', 'JSON must be a row of at most 4194304 characters.');
    endif
    source = model;
    try
      model = jsondecode(source, 'makeValidName', false);
    catch
      error('im:model:JSON', 'Invalid JSON text.');
    end_try_catch
    check_json_keys(source);
  endif
  fields_exact(model, {'schemaVersion','blocks','connections','simulation'}, {}, 'Schema', 'model');
  if !scalar_number(model.schemaVersion) || model.schemaVersion != 1
    error('im:model:Schema', 'schemaVersion must be the double scalar 1.');
  endif
  [sim, times] = simulation(model.simulation);
  raw = object_list(model.blocks, false, 'blocks');
  lines = object_list(model.connections, true, 'connections');
  if numel(raw)>256 || numel(lines)>4096
    error('im:model:Limit', 'Model exceeds 256 blocks or 4096 connections.');
  endif
  n = numel(raw); blocks = cell(1,n); ids = cell(1,n); x0 = [];
  for k=1:n
    b = raw{k};
    fields_exact(b, {'id','type','params','sampleTime'}, {'initialState'}, 'Schema', sprintf('block %d',k));
    if !ischar(b.id) || !isrow(b.id) || isempty(regexp(b.id, '^[A-Za-z][A-Za-z0-9_]{0,62}$', 'once'))
      error('im:model:Schema', 'Block %d id must match [A-Za-z][A-Za-z0-9_]{0,62}.', k);
    endif
    if any(strcmp(ids(1:k-1),b.id))
      error('im:model:Schema', 'Duplicate block id: %s.', b.id);
    endif
    ids{k}=b.id;
    if !ischar(b.type) || !isrow(b.type)
      error('im:model:Type', 'Block %s type must be a supported type name.', b.id);
    endif
    kinds = {'Constant','Step','Sine','Gain','Sum','Integrator','TransferFcn', ...
             'Saturation','Mux','Scope','UnitDelay','SampledInput','ClampingGate'};
    if !any(strcmp(b.type,kinds))
      error('im:model:Type', 'Block %s: unsupported type %s.', b.id,b.type);
    endif
    continuous = any(strcmp(b.type,{'Integrator','TransferFcn'}));
    discrete = any(strcmp(b.type,{'UnitDelay','SampledInput','ClampingGate'}));
    if (continuous && strcmp(sim.mode,'discrete')) || (discrete && strcmp(sim.mode,'continuous'))
      error('im:model:Mode', 'Block %s (%s) is not supported in %s mode; mixed models are unsupported.', b.id,b.type,sim.mode);
    endif
    expected=0; if strcmp(sim.mode,'discrete'), expected=sim.Ts; endif
    if !scalar_number(b.sampleTime) || b.sampleTime != expected
      error('im:model:SampleTime', 'Block %s sampleTime must equal %.17g; inherited and multiple rates are unsupported.', b.id,expected);
    endif
    has_ic = any(strcmp(b.type,{'Integrator','UnitDelay'}));
    if has_ic
      if !isfield(b,'initialState') || !scalar_number(b.initialState)
        error('im:model:State', 'Block %s requires a finite real double scalar initialState.', b.id);
      endif
    elseif isfield(b,'initialState')
      error('im:model:State', 'Block %s does not accept initialState; TransferFcn initial states are zero.', b.id);
    endif
    b.nin=0; b.nout=1; b.width=1; b.state=[]; b.A=[]; b.B=[]; b.C=[];
    switch b.type
      case 'Constant'
        b.params = parameters(b.params, {'value'}, {}, b.id);
      case 'Step'
        b.params = parameters(b.params, {'time','before','after'}, {}, b.id);
      case 'Sine'
        b.params = parameters(b.params, {'amplitude','omega','phase','bias'}, {}, b.id);
      case 'Gain'
        b.params = parameters(b.params, {'gain'}, {}, b.id); b.nin=1;
      case 'Sum'
        b.params = parameters(b.params, {}, {'signs'}, b.id);
        if !all(abs(b.params.signs)==1)
          error('im:model:Parameter', 'Block %s signs must contain only +1 or -1.',b.id);
        endif
        b.nin=numel(b.params.signs);
      case {'Saturation','ClampingGate'}
        b.params = parameters(b.params, {'lower','upper'}, {}, b.id);
        if b.params.lower > b.params.upper
          error('im:model:Parameter', 'Block %s requires lower <= upper.',b.id);
        endif
        b.nin=1+strcmp(b.type,'ClampingGate');
      case {'Integrator','UnitDelay'}
        b.params = parameters(b.params, {}, {}, b.id); b.nin=1;
        b.state=numel(x0)+1; x0(end+1,1)=b.initialState;
      case 'TransferFcn'
        b.params = parameters(b.params, {}, {'num','den'}, b.id); b.nin=1;
        num=b.params.num; den=b.params.den; degree=numel(den)-1;
        if degree<1 || numel(num)>=numel(den) || den(1)==0 || (num(1)==0 && numel(num)>1)
          error('im:model:Parameter', 'Block %s requires strictly proper num/den vectors without leading zeros (scalar zero numerator allowed).',b.id);
        endif
        if degree>64
          error('im:model:Limit', 'Block %s TransferFcn degree exceeds 64.', b.id);
        endif
        num=num/den(1); den=den/den(1);
        if any(!isfinite([num den]))
          error('im:model:Parameter', 'Block %s TransferFcn normalization is nonfinite.',b.id);
        endif
        b.A=[-den(2:end); eye(degree-1), zeros(degree-1,1)];
        b.B=[1; zeros(degree-1,1)]; b.C=[zeros(1,degree-numel(num)),num];
        b.state=numel(x0)+(1:degree); x0(b.state,1)=0;
      case 'Mux'
        b.params = parameters(b.params, {'inputs'}, {}, b.id);
        if b.params.inputs<1 || b.params.inputs!=fix(b.params.inputs)
          error('im:model:Parameter', 'Block %s inputs must be a positive integer.',b.id);
        endif
        b.nin=b.params.inputs; b.width=b.nin;
      case 'Scope'
        b.params = parameters(b.params, {}, {}, b.id); b.nin=1; b.nout=0;
      case 'SampledInput'
        b.params = parameters(b.params, {}, {'values'}, b.id);
        if numel(b.params.values)!=numel(times)
          error('im:model:Parameter', 'Block %s SampledInput requires exactly %d values for the simulation grid.', b.id,numel(times));
        endif
    endswitch
    if b.nin>256
      error('im:model:Limit', 'Block %s exceeds 256 input ports.', b.id);
    endif
    b.inputs=zeros(1,b.nin); blocks{k}=b;
  endfor
  edges=zeros(0,3);
  for j=1:numel(lines)
    link=lines{j};
    fields_exact(link, {'from','to'}, {}, 'Connection', sprintf('connection %d',j));
    [src,sp]=endpoint(link.from, ids, j, 'from');
    [dst,dp]=endpoint(link.to, ids, j, 'to');
    if sp>blocks{src}.nout || dp>blocks{dst}.nin
      error('im:model:Port', 'Connection %d: port out of range (%s.out%d -> %s.in%d).',j,ids{src},sp,ids{dst},dp);
    endif
    if blocks{dst}.inputs(dp)!=0
      error('im:model:Connection', 'Block %s input %d has more than one driver.',ids{dst},dp);
    endif
    if strcmp(blocks{src}.type,'Mux') && !strcmp(blocks{dst}.type,'Scope')
      error('im:model:Dimension', 'Mux %s may connect only to a Scope input.',ids{src});
    endif
    blocks{dst}.inputs(dp)=src;
    if !any(strcmp(blocks{dst}.type,{'Integrator','TransferFcn','UnitDelay'}))
      edges(end+1,:)=[src dst dp];
    endif
  endfor
  logged=0;
  for k=1:n
    p=find(blocks{k}.inputs==0,1);
    if !isempty(p)
      error('im:model:Connection', 'Block %s input %d has no driver.',ids{k},p);
    endif
    if strcmp(blocks{k}.type,'Scope')
      blocks{k}.width=blocks{blocks{k}.inputs(1)}.width;
      logged+=blocks{k}.width;
    endif
  endfor
  if numel(times)*(logged+numel(x0))>5e6
    error('im:model:Limit', 'Logged signals plus states exceed 5000000 output doubles.');
  endif
  order=topological(n,edges,ids);
  plan=struct('model',model,'blocks',{blocks},'order',order, ...
              'simulation',sim,'t',times,'x0',x0(:));
endfunction

function [sim,t] = simulation(sim)
  if !isstruct(sim) || !isscalar(sim) || !isfield(sim,'mode') || !ischar(sim.mode) || !isrow(sim.mode)
    error('im:model:Mode', 'simulation.mode must be continuous or discrete.');
  endif
  if strcmp(sim.mode,'continuous')
    fields_exact(sim, {'mode','t0','tf','solver','outputDt','relTol','absTol','maxStep'}, {}, 'Schema','simulation');
    step=sim.outputDt; expected='ode45';
    for key={'relTol','absTol','maxStep'}
      if !scalar_number(sim.(key{1})) || sim.(key{1})<=0
        error('im:model:Time', 'simulation.%s must be a positive finite real double scalar.',key{1});
      endif
    endfor
  elseif strcmp(sim.mode,'discrete')
    fields_exact(sim, {'mode','t0','tf','solver','Ts'}, {}, 'Schema','simulation');
    step=sim.Ts; expected='discrete';
  else
    error('im:model:Mode', 'Unsupported simulation mode; only continuous and single-rate discrete are supported.');
  endif
  if !ischar(sim.solver) || !strcmp(sim.solver,expected)
    error('im:model:Solver', 'Mode %s requires solver %s; other solvers are unsupported.',sim.mode,expected);
  endif
  if !scalar_number(sim.t0) || !scalar_number(sim.tf) || sim.tf<=sim.t0 || !scalar_number(step) || step<=0
    error('im:model:Time', 'Require finite real double t0 < tf and positive outputDt or Ts.');
  endif
  ratio=(sim.tf-sim.t0)/step; count=round(ratio);
  if !isfinite(ratio) || count+1>100001
    error('im:model:Limit', 'Simulation grid exceeds 100001 output points.');
  endif
  if count<1 || abs(ratio-count)>32*eps(max(1,abs(ratio)))
    error('im:model:Time', 'Simulation duration must be an integer multiple of outputDt or Ts (32-ulp ratio tolerance).');
  endif
  t=sim.t0+(0:count)'*step; t(end)=sim.tf;
  if any(!isfinite(t)) || any(diff(t)<=0)
    error('im:model:Time', 'Simulation grid collapses at double precision.');
  endif
endfunction

function p = parameters(p, scalars, vectors, id)
  fields_exact(p, [scalars vectors], {}, 'Parameter', ['block ' id ' params']);
  for j=1:numel(scalars)
    key=scalars{j};
    if !scalar_number(p.(key))
      error('im:model:Parameter', 'Block %s parameter %s must be a finite real double scalar.',id,key);
    endif
  endfor
  for j=1:numel(vectors)
    key=vectors{j}; v=p.(key);
    if !isa(v,'double') || !isreal(v) || issparse(v) || isempty(v) || !isvector(v) || any(!isfinite(v(:)))
      error('im:model:Parameter', 'Block %s parameter %s must be a nonempty finite real double vector.',id,key);
    endif
    p.(key)=v(:)';
  endfor
endfunction

function yes=scalar_number(v)
  yes=isa(v,'double') && isscalar(v) && isreal(v) && !issparse(v) && isfinite(v);
endfunction

function fields_exact(s, required, optional, code, context)
  if !isstruct(s) || !isscalar(s)
    error(['im:model:' code], '%s must be a scalar struct/object.',context);
  endif
  fields=fieldnames(s);
  for j=1:numel(required)
    if !isfield(s,required{j})
      error(['im:model:' code], '%s missing field %s.',context,required{j});
    endif
  endfor
  for j=1:numel(fields)
    if !any(strcmp(fields{j},[required optional]))
      error(['im:model:' code], '%s has unsupported field %s.',context,fields{j});
    endif
  endfor
endfunction

function list=object_list(v, allow_empty, context)
  if allow_empty && isempty(v) && (iscell(v) || isstruct(v) || isa(v,'double'))
    list={}; return;
  endif
  if isempty(v) || !isvector(v) || !(iscell(v) || isstruct(v))
    error('im:model:Schema', '%s must be a nonempty vector of objects.',context);
  endif
  if isstruct(v), list=num2cell(v(:)'); else, list=v(:)'; endif
  for j=1:numel(list)
    if !isstruct(list{j}) || !isscalar(list{j})
      error('im:model:Schema', '%s element %d must be a scalar object.',context,j);
    endif
  endfor
endfunction

function [index,port]=endpoint(p,ids,line,direction)
  fields_exact(p, {'block','port'}, {}, 'Connection',sprintf('connection %d %s',line,direction));
  if !ischar(p.block) || !isrow(p.block)
    error('im:model:Connection', 'Connection %d %s block must be an id.',line,direction);
  endif
  index=find(strcmp(ids,p.block));
  if isempty(index)
    error('im:model:Connection', 'Connection %d references unknown block %s.',line,p.block);
  endif
  port=p.port;
  if !scalar_number(port) || port<1 || port!=fix(port)
    error('im:model:Port', 'Connection %d %s port must be a positive integer double.',line,direction);
  endif
endfunction

function order=topological(n,edges,ids)
  degree=zeros(1,n);
  for j=1:rows(edges), degree(edges(j,2))+=1; endfor
  order=[]; ready=find(degree==0);
  while !isempty(ready)
    k=ready(1); ready(1)=[]; order(end+1)=k;
    for j=find(edges(:,1)==k)'
      to=edges(j,2); degree(to)-=1;
      if degree(to)==0, ready(end+1)=to; endif
    endfor
  endwhile
  if numel(order)==n, return; endif
  active=degree>0; chain=find(active,1);
  while true
    row=find(edges(:,2)==chain(end) & active(edges(:,1))',1);
    source=edges(row,1); previous=find(chain==source,1);
    if !isempty(previous)
      cycle=fliplr([chain(previous:end) source]); break;
    endif
    chain(end+1)=source;
  endwhile
  parts=cell(1,numel(cycle)-1);
  for j=1:numel(parts)
    row=find(edges(:,1)==cycle(j) & edges(:,2)==cycle(j+1),1);
    parts{j}=sprintf('%s.out1 -> %s.in%d',ids{cycle(j)},ids{cycle(j+1)},edges(row,3));
  endfor
  error('im:model:AlgebraicLoop', 'Algebraic loop: %s. Add no implicit delay; this model is unsupported.',strjoin(parts,'; '));
endfunction

function check_json_keys(source)
  % jsondecode validates syntax first. Inspect only tokens to reject duplicate
  % decoded object keys, including escaped-equivalent keys; no model eval.
  tokens=regexp(source, '"(?:\\.|[^"\\])*"|[{}\[\]:,]|[^\s{}\[\]:,"]+', 'match');
  stack={};
  for j=1:numel(tokens)
    token=tokens{j};
    if strcmp(token,'{')
      stack{end+1}=struct('object',true,'key',true,'keys',{{}});
    elseif strcmp(token,'[')
      stack{end+1}=struct('object',false,'key',false,'keys',{{}});
    elseif any(strcmp(token,{'}',']'}))
      stack(end)=[];
    elseif strcmp(token,',')
      if !isempty(stack) && stack{end}.object, stack{end}.key=true; endif
    elseif !isempty(stack) && stack{end}.object && stack{end}.key && token(1)=='"'
      key=jsondecode(token);
      if any(strcmp(stack{end}.keys,key))
        error('im:model:JSON', 'Duplicate JSON object key: %s.',key);
      endif
      stack{end}.keys{end+1}=key; stack{end}.key=false;
    endif
  endfor
endfunction
