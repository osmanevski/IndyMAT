function result = im_simulate(model)
  %IM_SIMULATE Run a validated IndyMAT block model in a local workspace.
  % result.t is the requested time grid, result.states has one row per time,
  % and result.signals.<ScopeID> contains the corresponding Scope channels.
  % Continuous states use ode45; discrete states are logged before updating.
  % This is a narrow block-model API, not a Simulink file interpreter.
  plan = im_compile(model);
  nt = numel(plan.t);
  nx = numel(plan.x0);
  result = struct('t', plan.t, 'signals', struct(), 'states', zeros(nt, nx));
  scopes = [];
  for i = 1:numel(plan.blocks)
    b = plan.blocks{i};
    if strcmp(b.type, 'Scope')
      scopes(end + 1) = i;
      result.signals.(b.id) = zeros(nt, b.width);
    end
  end

  if strcmp(plan.simulation.mode, 'continuous') && nx > 0
    result.states = continuous_states(plan);
  end
  x = plan.x0;
  for k = 1:nt
    if strcmp(plan.simulation.mode, 'continuous')
      x = result.states(k, :).';
    else
      result.states(k, :) = x.';
    end
    values = evaluate(plan, plan.t(k), x, k, plan.t(k));
    for i = scopes
      result.signals.(plan.blocks{i}.id)(k, :) = values{i};
    end
    if strcmp(plan.simulation.mode, 'discrete') && k < nt
      % Read every input from the same old-state graph, then commit once.
      next = x;
      for i = 1:numel(plan.blocks)
        b = plan.blocks{i};
        if strcmp(b.type, 'UnitDelay')
          next(b.state) = values{b.inputs(1)};
        end
      end
      finite_value(next, 'discrete state');
      x = next;
    end
  end
end

function states = continuous_states(plan)
  sim = plan.simulation;
  edges = [sim.t0; sim.tf];
  for i = 1:numel(plan.blocks)
    b = plan.blocks{i};
    if strcmp(b.type, 'Step') && b.params.time > sim.t0 && b.params.time < sim.tf
      edges(end + 1, 1) = b.params.time;
    end
  end
  edges = unique(edges);
  states = zeros(numel(plan.t), numel(plan.x0));
  states(1, :) = plan.x0.';
  x = plan.x0;
  options = odeset('RelTol', sim.relTol, 'AbsTol', sim.absTol, ...
                   'MaxStep', sim.maxStep);
  for segment = 1:numel(edges) - 1
    left = edges(segment);
    right = edges(segment + 1);
    rows = find(plan.t >= left & plan.t <= right);
    requested = unique([left; plan.t(rows); right]);
    % Freeze only Step sources to this segment's right-hand start value.
    % In particular, the RHS at its end still sees the LEFT limit of a
    % Step at that end. Sine and other time-dependent sources use actual t.
    rhs = @(t, state) derivatives(plan, t, state, left);
    try
      [sol_t, sol_x] = ode45(rhs, requested, x, options);
    catch err
      % Octave's adaptive solver exhaustion currently has no identifier.
      % Match that specific failure, leaving interrupts and unrelated
      % errors (including Nonfinite from our RHS) untouched.
      if isempty(err.identifier) && ~isempty(err.stack) && ...
          strcmp(err.stack(1).name, 'integrate_adaptive') && ...
          strncmp(err.message, 'integrate_adaptive: Solving was not successful.', 47)
        error('im:model:Solver', 'ode45 failed: %s', err.message);
      end
      rethrow(err);
    end
    finite_value(sol_x, 'continuous state');
    if isempty(sol_t) || sol_t(1) ~= left || sol_t(end) ~= right || ...
        size(sol_x, 1) ~= numel(sol_t) || size(sol_x, 2) ~= numel(x)
      error('im:model:Solver', 'ode45 did not cover the complete time segment.');
    end
    % A two-point tspan returns adaptive intermediate points in Octave.
    % Select exact requested times; never interpolate across a Step.
    [present, positions] = ismember(plan.t(rows), sol_t);
    if ~all(present)
      error('im:model:Solver', 'ode45 omitted a requested output time.');
    end
    states(rows, :) = sol_x(positions, :);
    x = sol_x(end, :).';
  end
end

function dx = derivatives(plan, t, x, step_time)
  values = evaluate(plan, t, x, 1, step_time);
  dx = zeros(size(x));
  for i = 1:numel(plan.blocks)
    b = plan.blocks{i};
    if strcmp(b.type, 'Integrator')
      dx(b.state) = values{b.inputs(1)};
    elseif strcmp(b.type, 'TransferFcn')
      dx(b.state) = b.A * x(b.state) + b.B * values{b.inputs(1)};
    end
  end
  finite_value(dx, 'state derivative');
end

function values = evaluate(plan, t, x, sample, step_time)
  finite_value(x, 'state');
  values = cell(1, numel(plan.blocks));
  for i = plan.order
    b = plan.blocks{i};
    p = b.params;
    switch b.type
      case 'Constant'
        y = p.value;
      case 'Step'
        if step_time < p.time
          y = p.before;
        else
          y = p.after;
        end
      case 'Sine'
        y = p.bias + p.amplitude * sin(p.omega * t + p.phase);
      case 'Gain'
        y = p.gain * values{b.inputs(1)};
      case 'Sum'
        y = sum(p.signs .* [values{b.inputs}]);
      case 'Saturation'
        y = min(p.upper, max(p.lower, values{b.inputs(1)}));
      case {'Integrator', 'UnitDelay'}
        y = x(b.state);
      case 'TransferFcn'
        y = b.C * x(b.state);
      case 'Mux'
        y = [values{b.inputs}];
      case 'Scope'
        y = values{b.inputs(1)};
      case 'SampledInput'
        y = p.values(sample);
      case 'ClampingGate'
        e = values{b.inputs(1)};
        raw = values{b.inputs(2)};
        if (raw > p.upper && e > 0) || (raw < p.lower && e < 0)
          y = 0;
        else
          y = e;
        end
      otherwise
        error('im:model:Solver', 'Unexpected compiled block type: %s.', b.type);
    end
    finite_value(y, ['block ' b.id]);
    values{i} = y;
  end
end

function finite_value(value, context)
  if any(~isfinite(value(:))) || ~isreal(value)
    error('im:model:Nonfinite', 'Nonfinite or nonreal value in %s.', context);
  end
end
