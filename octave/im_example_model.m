function model = im_example_model(name, parameters)
  %IM_EXAMPLE_MODEL Five small equation netlists; no graphics or package loading.
  % Names match the original workspace/examples scripts. Optional parameters:
  % PID: gains=[Kp Ki Kd]; discrete control and anti-windup: Ts.
  % Filtering requires values,b_fir,a_fir,b_iir,a_iir,Ts from the caller.
  % Outputs are logged by Scope 'output'; anti-windup also logs 'actuator'
  % and filtering logs 'input'. This is not a Simulink model importer.
  if nargin < 2, parameters = struct(); end
  if ~ischar(name) || ~isrow(name) || ~isstruct(parameters) || ~isscalar(parameters)
    error('im:example:Parameters', 'Name must be text and parameters a scalar struct.');
  end
  blocks = {}; connections = {}; Ts = 0;
  switch name
    case 'pid_kapali_cevrim'
      check_options({'gains'});
      gains = option('gains', [2 1 1]);
      if ~isa(gains,'double') || ~isreal(gains) || numel(gains) ~= 3 || any(~isfinite(gains(:)))
        error('im:example:Parameters', 'gains must contain three finite real doubles.');
      end
      simulation = continuous(40, 1e-9, 1e-11);
      add('reference','Step',struct('time',0,'before',0,'after',1));
      add('error','Sum',struct('signs',[1 -1]));
      add('proportional','Gain',struct('gain',gains(1)));
      add('integral','Integrator',struct(),0);
      add('integral_gain','Gain',struct('gain',gains(2)));
      % Kd*s/(Tf*s+1) = Kd/Tf * (1 - 1/(Tf*s+1)).
      add('derivative_filter','TransferFcn',struct('num',1,'den',[0.1 1]));
      add('derivative_error','Sum',struct('signs',[1 -1]));
      add('derivative_gain','Gain',struct('gain',gains(3)/0.1));
      add('control','Sum',struct('signs',[1 1 1]));
      add('plant','TransferFcn',struct('num',1,'den',[1 2 1]));
      add('output','Scope',struct());
      wire('reference','error',1); wire('plant','error',2);
      wire('error','proportional',1); wire('error','integral',1);
      wire('integral','integral_gain',1); wire('error','derivative_filter',1);
      wire('error','derivative_error',1); wire('derivative_filter','derivative_error',2);
      wire('derivative_error','derivative_gain',1);
      wire('proportional','control',1); wire('integral_gain','control',2);
      wire('derivative_gain','control',3); wire('control','plant',1);
      wire('plant','output',1);
    case 'kutle_yay_sonum'
      check_options({}); simulation = continuous(10, 1e-10, 1e-12);
      add('force','Constant',struct('value',0));
      add('force_balance','Sum',struct('signs',[1 -1 -1]));
      add('inverse_mass','Gain',struct('gain',1));
      add('velocity','Integrator',struct(),0);
      add('position','Integrator',struct(),1);
      add('damping','Gain',struct('gain',0.8));
      add('spring','Gain',struct('gain',4));
      add('output','Scope',struct());
      wire('force','force_balance',1); wire('damping','force_balance',2);
      wire('spring','force_balance',3); wire('force_balance','inverse_mass',1);
      wire('inverse_mass','velocity',1); wire('velocity','position',1);
      wire('velocity','damping',1); wire('position','spring',1);
      wire('position','output',1);
    case 'ayrik_kontrol'
      check_options({'Ts'}); Ts = option('Ts',0.05); simulation = discrete(8,Ts);
      a = exp(-Ts);
      add('reference','Step',struct('time',0,'before',0,'after',1));
      add('error','Sum',struct('signs',[1 -1]));
      add('controller','Gain',struct('gain',2));
      add('input_gain','Gain',struct('gain',1-a));
      add('old_gain','Gain',struct('gain',a));
      add('next','Sum',struct('signs',[1 1]));
      add('plant','UnitDelay',struct(),0); add('output','Scope',struct());
      wire('reference','error',1); wire('plant','error',2);
      wire('error','controller',1); wire('controller','input_gain',1);
      wire('plant','old_gain',1); wire('input_gain','next',1);
      wire('old_gain','next',2); wire('next','plant',1); wire('plant','output',1);
    case 'doyum_antiwindup'
      check_options({'Ts'}); Ts = option('Ts',0.002); simulation = discrete(12,Ts);
      add('reference','Step',struct('time',5,'before',2,'after',0.5));
      add('outputs','Mux',struct('inputs',2)); add('output','Scope',struct());
      add('actuators','Mux',struct('inputs',2)); add('actuator','Scope',struct());
      wire('outputs','output',1); wire('actuators','actuator',1);
      for j = 1:2
        p = sprintf('pi%d_',j);
        add([p 'plant'],'UnitDelay',struct(),0);
        add([p 'integral'],'UnitDelay',struct(),0);
        add([p 'error'],'Sum',struct('signs',[1 -1]));
        add([p 'proportional'],'Gain',struct('gain',2));
        add([p 'integral_gain'],'Gain',struct('gain',1));
        add([p 'raw'],'Sum',struct('signs',[1 1]));
        add([p 'actuator'],'Saturation',struct('lower',-1,'upper',1));
        add([p 'plant_gain'],'Gain',struct('gain',exp(-Ts)));
        add([p 'input_gain'],'Gain',struct('gain',1-exp(-Ts)));
        add([p 'plant_next'],'Sum',struct('signs',[1 1]));
        add([p 'integral_step'],'Gain',struct('gain',Ts));
        add([p 'integral_next'],'Sum',struct('signs',[1 1]));
        wire('reference',[p 'error'],1); wire([p 'plant'],[p 'error'],2);
        wire([p 'error'],[p 'proportional'],1);
        wire([p 'integral'],[p 'integral_gain'],1);
        wire([p 'proportional'],[p 'raw'],1); wire([p 'integral_gain'],[p 'raw'],2);
        wire([p 'raw'],[p 'actuator'],1);
        wire([p 'plant'],[p 'plant_gain'],1); wire([p 'actuator'],[p 'input_gain'],1);
        wire([p 'plant_gain'],[p 'plant_next'],1); wire([p 'input_gain'],[p 'plant_next'],2);
        wire([p 'plant_next'],[p 'plant'],1);
        if j == 2
          add([p 'clamping'],'ClampingGate',struct('lower',-1,'upper',1));
          wire([p 'error'],[p 'clamping'],1); wire([p 'raw'],[p 'clamping'],2);
          wire([p 'clamping'],[p 'integral_step'],1);
        else
          wire([p 'error'],[p 'integral_step'],1);
        end
        wire([p 'integral'],[p 'integral_next'],1);
        wire([p 'integral_step'],[p 'integral_next'],2);
        wire([p 'integral_next'],[p 'integral'],1);
        wire([p 'plant'],'outputs',j); wire([p 'actuator'],'actuators',j);
      end
    case 'sinyal_suzme'
      names = {'values','b_fir','a_fir','b_iir','a_iir','Ts'};
      check_options(names);
      if ~all(isfield(parameters,names))
        error('im:example:Parameters', 'Filtering requires values, both coefficient pairs and Ts.');
      end
      Ts = parameters.Ts; simulation = discrete((numel(parameters.values)-1)*Ts,Ts);
      add('samples','SampledInput',struct('values',parameters.values));
      add('input','Scope',struct()); wire('samples','input',1);
      add('filters','Mux',struct('inputs',2)); add('output','Scope',struct());
      wire('filters','output',1);
      filter_chain('fir',parameters.b_fir,parameters.a_fir,1);
      filter_chain('iir',parameters.b_iir,parameters.a_iir,2);
    otherwise
      error('im:example:Name', 'Unknown example name: %s', name);
  end
  model = struct('schemaVersion',1,'blocks',{blocks},'connections',{connections}, ...
                 'simulation',simulation);

  function check_options(allowed)
    if any(~ismember(fieldnames(parameters),allowed))
      error('im:example:Parameters', 'Unknown example parameter.');
    end
  end
  function value = option(key, fallback)
    if isfield(parameters,key), value = parameters.(key); else, value = fallback; end
  end
  function add(id,type,params,varargin)
    block = struct('id',id,'type',type,'params',params,'sampleTime',Ts);
    if ~isempty(varargin), block.initialState = varargin{1}; end
    blocks{end+1} = block;
  end
  function wire(from,to,port)
    connections{end+1} = struct('from',struct('block',from,'port',1), ...
                              'to',struct('block',to,'port',port));
  end
  function filter_chain(prefix,b,a,channel)
    if ~isa(b,'double') || ~isa(a,'double') || ~isvector(b) || ~isvector(a) || ...
       isempty(b) || isempty(a) || ~isreal(b) || ~isreal(a) || ...
       any(~isfinite(b(:))) || any(~isfinite(a(:))) || a(1) == 0
      error('im:example:Parameters', 'Filter coefficients must be finite real vectors with a(1) nonzero.');
    end
    % Direct difference equation: sum b(k)*x[n-k] - sum a(k)*y[n-k].
    total = [prefix '_sum'];
    add(total,'Sum',struct('signs',ones(1,numel(b)+numel(a)-1)));
    source = 'samples';
    for k = 1:numel(b)
      if k > 1
        delay = sprintf('%s_x%d',prefix,k-1);
        add(delay,'UnitDelay',struct(),0); wire(source,delay,1); source = delay;
      end
      gain = sprintf('%s_b%d',prefix,k);
      add(gain,'Gain',struct('gain',b(k)/a(1))); wire(source,gain,1); wire(gain,total,k);
    end
    source = total;
    for k = 2:numel(a)
      delay = sprintf('%s_y%d',prefix,k-1);
      add(delay,'UnitDelay',struct(),0); wire(source,delay,1); source = delay;
      gain = sprintf('%s_a%d',prefix,k);
      add(gain,'Gain',struct('gain',-a(k)/a(1)));
      wire(source,gain,1); wire(gain,total,numel(b)+k-1);
    end
    wire(total,'filters',channel);
  end
end

function simulation = continuous(tf,relTol,absTol)
  simulation = struct('mode','continuous','t0',0,'tf',tf,'solver','ode45', ...
                      'outputDt',0.01,'relTol',relTol,'absTol',absTol,'maxStep',0.05);
end

function simulation = discrete(tf,Ts)
  if ~isa(Ts,'double') || ~isscalar(Ts) || ~isreal(Ts) || ~isfinite(Ts) || Ts <= 0
    error('im:example:Parameters', 'Ts must be a positive finite real double.');
  end
  simulation = struct('mode','discrete','t0',0,'tf',tf,'solver','discrete','Ts',Ts);
end
