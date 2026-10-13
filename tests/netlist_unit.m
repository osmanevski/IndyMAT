function netlist_unit(group)
  switch group
    case 'numeric', numeric_cases();
    case 'states', state_cases();
    case 'schema', schema_cases();
    case 'parameters', parameter_cases();
    case 'graph', graph_cases();
    case 'json', json_cases();
    case 'limits', limit_cases();
    case 'nonfinite', nonfinite_cases();
    otherwise, error('test:Group', 'Unknown test group');
  end
  fprintf('NETLIST PASS %s\n', group);
end

function b = block(id, type, params, ts, varargin)
  b = struct('id', id, 'type', type, 'params', params, 'sampleTime', ts);
  if !isempty(varargin), b.initialState = varargin{1}; end
end
function c = edge(src, dst, port)
  c = struct('from', struct('block', src, 'port', 1), ...
             'to', struct('block', dst, 'port', port));
end
function m = model(blocks, connections, discrete)
  if nargin < 3, discrete = false; end
  if discrete
    sim = struct('mode','discrete','t0',0,'tf',1,'solver','discrete','Ts',.1);
  else
    sim = struct('mode','continuous','t0',0,'tf',1,'solver','ode45', ...
      'outputDt',.1,'relTol',1e-9,'absTol',1e-11,'maxStep',.02);
  end
  m = struct('schemaVersion',1,'blocks',{blocks},'connections',{connections},'simulation',sim);
end
function m = constant_model(discrete)
  if nargin < 1, discrete = false; end
  ts = .1 * discrete;
  m = model({block('C','Constant',struct('value',2),ts), ...
    block('Out','Scope',struct(),ts)}, {edge('C','Out',1)}, discrete);
end
function fail(m, suffix, simulate)
  if nargin < 3, simulate = false; end
  try
    if simulate, im_simulate(m); else, im_compile(m); end
  catch err
    assert(strcmp(err.identifier,['im:model:' suffix]), ...
      sprintf('Expected %s, got %s: %s', suffix, err.identifier, err.message));
    return;
  end
  error('test:ExpectedError', 'Expected im:model:%s', suffix);
end
function close_to(actual, expected, tol)
  assert(isequal(size(actual),size(expected)), 'Unexpected output shape');
  assert(all(abs(actual(:)-expected(:)) <= tol), 'Numerical result differs');
end

function numeric_cases()
  m = constant_model(); r = im_simulate(m);
  close_to(r.t,(0:10)'*.1,1e-15); close_to(r.signals.Out,2*ones(11,1),0);
  assert(isempty(r.states));
  assert(isa(r.t,'double') && isa(r.signals.Out,'double') && isa(r.states,'double'));
  assignin('base','netlist_sentinel',struct('value',17,'handle',@(x)x+1));
  im_compile(m); im_simulate(m);
  sentinel=evalin('base','netlist_sentinel');
  assert(sentinel.value==17 && sentinel.handle(4)==5);
  evalin('base','clear netlist_sentinel');
  % Stateless continuous evaluation includes the exact right-continuous boundary.
  m.blocks{1}=block('C','Step',struct('time',.35,'before',-1,'after',3),0);
  r=im_simulate(m); close_to(r.signals.Out,-1+4*(r.t>=.35),0);
  m.blocks{1}.params.time=.4; r=im_simulate(m); assert(r.signals.Out(5)==3);
  m.blocks{1}.params.time=1; r=im_simulate(m); assert(r.signals.Out(end)==3);
  m.blocks{1}.params.time=0; r=im_simulate(m); assert(all(r.signals.Out==3));
  m.blocks{1}=block('C','Sine',struct('amplitude',2,'omega',3,'phase',.2,'bias',-1),0);
  r=im_simulate(m); close_to(r.signals.Out,2*sin(3*r.t+.2)-1,1e-14);
  m=model({block('A','Constant',struct('value',3),0), ...
    block('B','Constant',struct('value',2),0), ...
    block('G','Gain',struct('gain',-2),0), ...
    block('S','Sum',struct('signs',[1;-1]),0), ...
    block('L','Saturation',struct('lower',-5,'upper',5),0), ...
    block('M','Mux',struct('inputs',3),0),block('Out','Scope',struct(),0)}, ...
    {edge('A','G',1),edge('G','S',1),edge('B','S',2),edge('S','L',1), ...
     edge('S','M',1),edge('L','M',2),edge('B','M',3),edge('M','Out',1)});
  r=im_simulate(m); close_to(r.signals.Out,repmat([-8 -5 2],11,1),0);
  p=im_compile(m); assert(isequal(p.blocks{4}.params.signs,[1 -1]));
  % All equal struct shapes permit row/column struct-array input as well.
  m.blocks=[m.blocks{:}]; m.connections=[m.connections{:}];
  r=im_simulate(m); close_to(r.signals.Out,repmat([-8 -5 2],11,1),0);
  m.blocks=m.blocks(:); m.connections=m.connections(:); im_compile(m);
  m=constant_model(); m.blocks=m.blocks(:); m.connections=m.connections(:); im_compile(m);
  % Discrete input samples are consumed once, without interpolation.
  m=constant_model(true); m.blocks{1}=block('C','SampledInput',struct('values',(0:10)'),.1);
  r=im_simulate(m); close_to(r.signals.Out,(0:10)',0);
  % ClampingGate equality passes e; only outward error outside bounds is blocked.
  e=[1 1 -1 -1 1 -1 0 2 -2 3 -3]; raw=[2 1 -2 -1 -2 2 5 0 0 1 -1];
  m=model({block('E','SampledInput',struct('values',e),.1), ...
    block('R','SampledInput',struct('values',raw),.1), ...
    block('Gate','ClampingGate',struct('lower',-1,'upper',1),.1), ...
    block('Out','Scope',struct(),.1)}, ...
    {edge('E','Gate',1),edge('R','Gate',2),edge('Gate','Out',1)},true);
  r=im_simulate(m); close_to(r.signals.Out,[0 1 0 -1 1 -1 0 2 -2 3 -3]',0);
end

function state_cases()
  m=model({block('U','Step',struct('time',.35,'before',0,'after',2),0), ...
    block('I','Integrator',struct(),0,3),block('Out','Scope',struct(),0)}, ...
    {edge('U','I',1),edge('I','Out',1)});
  r=im_simulate(m); close_to(r.signals.Out,3+2*max(r.t-.35,0),2e-8);
  close_to(r.states,r.signals.Out,1e-12);
  m.blocks{1}.params.time=.4;
  m.blocks{4}=block('Input','Scope',struct(),0); m.connections{3}=edge('U','Input',1);
  r=im_simulate(m);
  close_to(r.signals.Out,3+2*max(r.t-.4,0),2e-8);
  assert(r.signals.Input(5)==2);
  m.blocks{1}.params.time=1; r=im_simulate(m);
  close_to(r.signals.Out,3*ones(11,1),2e-8); assert(r.signals.Input(end)==2);
  m.blocks{1}.params.time=.4;
  % Two-point ode45 spans must still return exactly the two requested samples.
  m.simulation.outputDt=1; r=im_simulate(m);
  close_to(r.t,[0;1],0); close_to(r.signals.Out,[3;4.2],2e-8);
  m=model({block('U','Constant',struct('value',1),0), ...
    block('F','TransferFcn',struct('num',[1 2],'den',[1 3 2]),0), ...
    block('Out','Scope',struct(),0)}, {edge('U','F',1),edge('F','Out',1)});
  r=im_simulate(m); close_to(r.signals.Out,1-exp(-r.t),2e-8);
  assert(isequal(size(r.states),[11 2])); assert(all(r.states(1,:)==0));
  m.blocks{2}.params=struct('num',2,'den',[2 4]); r=im_simulate(m);
  close_to(r.signals.Out,.5*(1-exp(-2*r.t)),2e-8);
  m.blocks{2}.params=struct('num',1,'den',[1 2 1]); r=im_simulate(m);
  close_to(r.signals.Out,1-(1+r.t).*exp(-r.t),2e-8);
  m.blocks{2}.params=struct('num',0,'den',[2 4]); r=im_simulate(m);
  close_to(r.signals.Out,zeros(11,1),0);
  % State blocks break continuous feedback cycles without an algebraic solver.
  feedback=model({block('I','Integrator',struct(),0,1), ...
    block('G','Gain',struct('gain',-2),0),block('Out','Scope',struct(),0)}, ...
    {edge('I','G',1),edge('G','I',1),edge('I','Out',1)});
  r=im_simulate(feedback); close_to(r.signals.Out,exp(-2*r.t),2e-8);
  % A discrete cycle through delays uses old values for all state updates.
  m=model({block('A','UnitDelay',struct(),.1,1), ...
    block('B','UnitDelay',struct(),.1,2),block('M','Mux',struct('inputs',2),.1), ...
    block('Out','Scope',struct(),.1)}, ...
    {edge('A','B',1),edge('B','A',1),edge('A','M',1),edge('B','M',2),edge('M','Out',1)},true);
  r=im_simulate(m); expected=repmat([1 2],11,1); expected(2:2:end,:) = repmat([2 1],5,1);
  close_to(r.signals.Out,expected,0); close_to(r.states,expected,0);
  % Compilation plans cannot substitute for a model or bypass validation.
  p=im_compile(constant_model()); fail(p,'Schema',true);
  forged=constant_model(); forged.blocks{1}.params.value=NaN;
  fail(forged,'Parameter',true);
end

function schema_cases()
  m=constant_model(); fail([], 'Schema'); fail({m},'Schema'); fail([m m],'Schema');
  for f={'schemaVersion','blocks','connections','simulation'}
    fail(rmfield(m,f{1}),'Schema');
  end
  n=m; n.schemaVersion=2; fail(n,'Schema'); n.schemaVersion=single(1); fail(n,'Schema');
  n=m; n.layout=struct(); fail(n,'Schema');
  n=m; n.blocks={}; fail(n,'Schema'); n.blocks={m.blocks{1},42}; fail(n,'Schema');
  for f={'id','type','params','sampleTime'}
    n=m; n.blocks{1}=rmfield(n.blocks{1},f{1}); fail(n,'Schema');
  end
  for id={'1bad','has space','x-y',repmat('a',1,64),''}
    n=m; n.blocks{1}.id=id{1}; fail(n,'Schema');
  end
  n=m; n.blocks{2}.id='C'; fail(n,'Schema');
  n=m; n.blocks{1}.reset=true; fail(n,'Schema');
  n=m; n.blocks{1}.type='Reset'; fail(n,'Type');
  n=m; n.simulation.layout=0; fail(n,'Schema');
  n=m; n.simulation.mode='mixed'; fail(n,'Mode');
  n=m; n.simulation.solver='ode15s'; fail(n,'Solver');
  n=constant_model(true); n.simulation.solver='ode45'; fail(n,'Solver');
  for v={NaN,Inf,-1,.1,single(0)}
    n=m; n.blocks{1}.sampleTime=v{1}; fail(n,'SampleTime');
  end
  n=constant_model(true); n.blocks{1}.sampleTime=.2; fail(n,'SampleTime');
  n=constant_model(true); n.blocks{1}.sampleTime=0; fail(n,'SampleTime');
  for field={'t0','tf','outputDt','relTol','absTol','maxStep'}
    n=m; n.simulation.(field{1})=NaN; fail(n,'Time');
  end
  for field={'outputDt','relTol','absTol','maxStep'}
    n=m; n.simulation.(field{1})=0; fail(n,'Time');
  end
  n=m; n.simulation.tf=0; fail(n,'Time');
  n=m; n.simulation.outputDt=.3; fail(n,'Time');
  n=m; n.simulation.t0=1e16; n.simulation.tf=1e16+4; n.simulation.outputDt=1; fail(n,'Time');
end

function parameter_cases()
  m=constant_model();
  for v={NaN,Inf,1i,[1 2],single(1),true,'2'}
    n=m; n.blocks{1}.params.value=v{1}; fail(n,'Parameter');
  end
  n=m; n.blocks{1}.params.extra=1; fail(n,'Parameter');
  n=m; n.blocks{1}.params=struct(); fail(n,'Parameter');
  specs={ 'Step',struct('time',0,'before',0,'after',Inf); ...
    'Sine',struct('amplitude',1,'omega',NaN,'phase',0,'bias',0); ...
    'Gain',struct('gain',Inf); 'Sum',struct('signs',[1 0]); ...
    'Sum',struct('signs',[]); 'Sum',struct('signs',ones(2)); ...
    'Saturation',struct('lower',2,'upper',1); 'Mux',struct('inputs',1.5); ...
    'Mux',struct('inputs',0); 'TransferFcn',struct('num',[1 2],'den',[1 2]); ...
    'TransferFcn',struct('num',1,'den',[0 1]); ...
    'TransferFcn',struct('num',[0 1],'den',[1 2 3]); ...
    'TransferFcn',struct('num',1,'den',[]); ...
    'TransferFcn',struct('num',1,'den',[1 Inf]); ...
    'SampledInput',struct('values',ones(10,1)); ...
    'SampledInput',struct('values',[ones(10,1);NaN]); ...
    'ClampingGate',struct('lower',2,'upper',1)};
  for k=1:rows(specs)
    discrete=any(strcmp(specs{k,1},{'SampledInput','ClampingGate'}));
    n=constant_model(discrete); n.blocks{1}=block('C',specs{k,1},specs{k,2},.1*discrete);
    fail(n,'Parameter');
  end
  n=m; n.blocks{1}.initialState=0; fail(n,'State');
  n=m; n.blocks{1}=block('C','TransferFcn',struct('num',1,'den',[1 1]),0,0); fail(n,'State');
  for type={'Integrator','UnitDelay'}
    discrete=strcmp(type{1},'UnitDelay'); n=constant_model(discrete);
    n.blocks{1}=block('C',type{1},struct(),.1*discrete); fail(n,'State');
    for v={NaN,Inf,[1 2],single(0),1i,[]}
      n.blocks{1}.initialState=v{1}; fail(n,'State');
    end
  end
  for type={'Integrator','TransferFcn','UnitDelay','SampledInput','ClampingGate'}
    discrete=any(strcmp(type{1},{'Integrator','TransferFcn'})); n=constant_model(discrete);
    switch type{1}
      case 'TransferFcn', params=struct('num',1,'den',[1 1]);
      case 'SampledInput', params=struct('values',ones(11,1));
      case 'ClampingGate', params=struct('lower',-1,'upper',1);
      otherwise, params=struct();
    end
    n.blocks{1}=block('C',type{1},params,.1*discrete);
    if any(strcmp(type{1},{'Integrator','UnitDelay'})), n.blocks{1}.initialState=0; end
    fail(n,'Mode');
  end
end

function graph_cases()
  m=constant_model(); n=m; n.connections=[]; fail(n,'Connection');
  n=m; n.connections{2}=n.connections{1}; fail(n,'Connection');
  n=m; n.connections{1}.from.block='Missing'; fail(n,'Connection');
  for v={0,-1,1.5,NaN,Inf,single(1)}
    n=m; n.connections{1}.from.port=v{1}; fail(n,'Port');
  end
  n=m; n.connections{1}.from.port=2; fail(n,'Port');
  n=m; n.connections{1}.to.port=2; fail(n,'Port');
  n=m; n.connections{1}.from.block='Out'; fail(n,'Port');
  n=m; n.connections{1}.foo=1; fail(n,'Connection');
  n=m; n.connections{1}.from.foo=1; fail(n,'Connection');
  n=m; n.blocks{3}=block('G','Gain',struct('gain',2),0);
  n.connections{2}=edge('G','G',1); fail(n,'AlgebraicLoop');
  n.blocks{4}=block('H','Gain',struct('gain',3),0);
  n.connections{2}=edge('G','H',1); n.connections{3}=edge('H','G',1); fail(n,'AlgebraicLoop');
  try, im_compile(n); catch err
    assert(!isempty(strfind(err.message,'G')) && !isempty(strfind(err.message,'H')));
    assert(!isempty(strfind(err.message,'1')), 'Cycle error must include a port');
  end
  n=model({block('C','Constant',struct('value',1),0), ...
    block('M','Mux',struct('inputs',1),0),block('G','Gain',struct('gain',1),0), ...
    block('Out','Scope',struct(),0)}, ...
    {edge('C','M',1),edge('M','G',1),edge('G','Out',1)}); fail(n,'Dimension');
end

function json_cases()
  m=constant_model(); r=im_simulate(jsonencode(m)); close_to(r.signals.Out,2*ones(11,1),0);
  for txt={'{','[]','null','true','"model.json"','{"schemaVersion":1,}', ...
    '{"schemaVersion":1,"schemaVersion":1}', ...
    '{"schemaVersion":1,"schema\u0056ersion":1}'}
    if any(strcmp(txt{1},{'[]','null','true','"model.json"'})), id='Schema'; else, id='JSON'; end
    fail(txt{1},id);
  end
  txt=jsonencode(m);
  fail(strrep(txt,'"schemaVersion":1','"schemaVersion":1,"bad-key":2'),'Schema');
  fail(strrep(txt,'"value":2','"value":2,"value":3'),'JSON');
  fail(strrep(txt,'"value":2','"value":null'),'Parameter');
  fail(strrep(txt,'"id":"C"','"id":"C","i\u0064":"D"'),'JSON');
  fail('netlist.json','JSON');
end

function limit_cases()
  n=constant_model(); n.simulation.outputDt=1e-5; p=im_compile(n); assert(numel(p.t)==100001);
  n=constant_model(); n.blocks{1}=block('C','TransferFcn',struct('num',1,'den',ones(1,66)),0); fail(n,'Limit');
  n=constant_model(); n.blocks{1}=block('C','Mux',struct('inputs',257),0); fail(n,'Limit');
  n=constant_model(); n.blocks{1}=block('C','Sum',struct('signs',ones(1,257)),0); fail(n,'Limit');
  n=constant_model(); n.simulation.outputDt=1e-5; n.simulation.tf=2; fail(n,'Limit');
  n=constant_model();
  for k=3:257, n.blocks{k}=block(sprintf('C%d',k),'Constant',struct('value',0),0); end
  fail(n,'Limit');
  n=constant_model(); n.connections=repmat(n.connections,1,4097); fail(n,'Limit');
  % One Mux logs 100 channels at 100001 points: exceeds doubles budget.
  n=constant_model(); n.simulation.outputDt=1e-5;
  n.blocks{3}=block('M','Mux',struct('inputs',100),0); n.connections={edge('M','Out',1)};
  for k=1:100, n.connections{end+1}=edge('C','M',k); end
  fail(n,'Limit');
  % State storage also counts even when disconnected from the Scope.
  n=constant_model(); n.simulation.outputDt=1e-5;
  for k=1:50
    id=sprintf('I%d',k); n.blocks{end+1}=block(id,'Integrator',struct(),0,0);
    n.connections{end+1}=edge('C',id,1);
  end
  fail(n,'Limit');
end

function nonfinite_cases()
  m=model({block('C','Constant',struct('value',realmax),0), ...
    block('G','Gain',struct('gain',2),0),block('Out','Scope',struct(),0)}, ...
    {edge('C','G',1),edge('G','Out',1)}); fail(m,'Nonfinite',true);
  % Unobserved branches are evaluated and checked too.
  m=constant_model(); m.blocks{3}=block('Huge','Constant',struct('value',realmax),0);
  m.blocks{4}=block('Bad','Gain',struct('gain',2),0);
  m.connections{2}=edge('Huge','Bad',1); fail(m,'Nonfinite',true);
  m=model({block('C','Constant',struct('value',0),.1), ...
    block('D','UnitDelay',struct(),.1,realmax),block('G','Gain',struct('gain',2),.1), ...
    block('Out','Scope',struct(),.1)}, ...
    {edge('D','G',1),edge('G','D',1),edge('C','Out',1)},true); fail(m,'Nonfinite',true);
end
