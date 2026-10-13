function netlist_numerical(root)
  % Original numeric cores execute in a function workspace, with no figures.
  ref = original(root,'pid_kapali_cevrim');
  for j = 1:3
    model = im_example_model('pid_kapali_cevrim',struct('gains',ref.kazanclar(j,:)));
    result = im_simulate(model); v = result.signals.output;
    near(result.t,ref.t,1e-12,'PID time grid');
    near(v,ref.y(:,j),2e-7,'PID response');
    if ref.kazanclar(j,2) == 0, final = 2/3; else, final = 1; end
    metrics = response_metrics(result.t,v,final);
    expected = response_metrics(ref.t,ref.y(:,j),final);
    assert(all(abs(metrics-expected) <= [0.02 0.001 2e-7]));
    assert(abs(metrics(3)-(1-final)) <= 0.001);
  end
  ref = original(root,'kutle_yay_sonum');
  result = im_simulate(im_example_model('kutle_yay_sonum'));
  near(result.t,ref.t,1e-12,'mass time grid');
  near(result.signals.output,ref.y_ss,1e-8,'mass state-space reference');
  near(result.signals.output,ref.x_ode(:,1),1e-8,'mass ODE reference');
  near(result.signals.output,ref.y_analitik,1e-8,'mass analytical reference');

  ref = original(root,'ayrik_kontrol');
  for j = 1:2
    result = im_simulate(im_example_model('ayrik_kontrol',struct('Ts',ref.Tsler(j))));
    near(result.t,ref.zaman{j},1e-12,'discrete time grid');
    near(result.signals.output,ref.cikis{j},1e-10,'discrete ZOH reference');
    a = exp(-ref.Tsler(j)); pole = a-2*(1-a);
    analytical = 2/3*(1-pole.^(0:numel(result.t)-1)');
    near(result.signals.output,analytical,1e-10,'discrete closed formula');
    continuous = 2/3*(1-exp(-3*result.t));
    difference = max(abs(result.signals.output-continuous));
    expected = max(abs(analytical-continuous));
    assert(abs(difference-expected) <= 1e-10);
  end

  ref = original(root,'doyum_antiwindup'); results = cell(1,2);
  for q = 1:2
    results{q} = im_simulate(im_example_model('doyum_antiwindup',struct('Ts',ref.hler(q))));
    result = results{q};
    near(result.t,ref.zamanlar{q},1e-12,'anti-windup time grid');
    near(result.signals.output,ref.sonuclar{q},1e-10,'anti-windup state recurrence');
    near(result.signals.actuator,ref.actuators{q},1e-10,'anti-windup actuator recurrence');
    first = result.t <= 5;
    analytical = (1-exp(-result.t(first)))*ones(1,2);
    near(result.signals.output(first,:),analytical,1e-10,'saturated analytical phase');
    assert(max(abs(result.signals.actuator(:))) <= 1+1e-12);
  end
  coarse = results{1}; refined = results{2}.signals.output(1:2:end,:);
  difference = max(max(abs(coarse.signals.output-refined)));
  assert(difference <= 0.005);
  after = coarse.t >= 5;
  area = trapz(coarse.t(after),abs(coarse.signals.output(after,:)-0.5));
  area_ref = trapz(coarse.t(after),abs(refined(after,:)-0.5));
  assert(all(abs(area-area_ref) <= 0.01)); assert(area(2) < area(1));
  near(area,ref.alan,1e-10,'anti-windup absolute error area');
  assert(abs(difference-ref.fark) <= 1e-10);

  % Pass exactly the x and filter coefficients produced by the original core.
  ref = original(root,'sinyal_suzme'); before = rng();
  options = struct('values',ref.x,'b_fir',ref.b_fir,'a_fir',ref.a_fir, ...
                   'b_iir',ref.b_iir,'a_iir',ref.a_iir,'Ts',1/ref.Fs);
  model = im_example_model('sinyal_suzme',options);
  assert(numel(model.blocks) < 160);
  result = im_simulate(model); assert(isequal(before,rng()));
  near(result.t,ref.t,1e-12,'filter time grid');
  near(result.signals.input,ref.x,0,'filter input is original sampled record');
  near(result.signals.output,ref.y,1e-10,'FIR and IIR difference equations');
  X = fft([ref.x result.signals.output]); spectrum = abs(X(1:ref.N/2+1,:))/ref.N;
  spectrum(2:end-1,:) = 2*spectrum(2:end-1,:);
  near(spectrum,ref.spektrum,1e-10,'filter amplitude spectrum');
  energy_error = max(abs(sum(abs(X).^2)/ref.N-sum([ref.x result.signals.output].^2)));
  assert(energy_error <= 1e-8);
  fprintf('NETLIST NUMERICAL OK: five examples, all parameter runs and reference metrics\n');
end

function ref = original(root,name)
  source = fileread(fullfile(root,'workspace','examples',[name '.m']));
  marker = '%% Grafikler — sayısal test bu işaretten önce durur';
  location = strfind(source,marker); assert(numel(location) == 1);
  core = source(1:location-1);
  assert(isempty(regexp(core,'\<(figure|plot|subplot|close)\s*\(', 'once')));
  eval(core);
  switch name
    case 'pid_kapali_cevrim'
      ref = struct('t',t,'y',y,'kazanclar',kazanclar);
    case 'kutle_yay_sonum'
      ref = struct('t',t,'y_ss',y_ss,'x_ode',x_ode,'y_analitik',y_analitik);
    case 'ayrik_kontrol'
      ref = struct('Tsler',Tsler,'zaman',{zaman},'cikis',{cikis});
    case 'doyum_antiwindup'
      ref = struct('hler',hler,'zamanlar',{zamanlar},'sonuclar',{sonuclar}, ...
                   'actuators',{{u_grafik,u}},'alan',alan,'fark',fark);
    case 'sinyal_suzme'
      ref = struct('t',t,'x',x,'y',y,'b_fir',b_fir,'a_fir',a_fir, ...
                   'b_iir',b_iir,'a_iir',a_iir,'Fs',Fs,'N',N,'spektrum',spektrum);
  end
end

function near(actual,expected,tolerance,label)
  assert(isequal(size(actual),size(expected)),[label ': dimensions differ']);
  difference = max(abs(actual(:)-expected(:)));
  assert(difference <= tolerance,sprintf('%s: error %.17g exceeds %.17g',label,difference,tolerance));
  fprintf('%s: max error=%.6g, tolerance=%.6g\n',label,difference,tolerance);
end

function metrics = response_metrics(t,v,final)
  low = find(v >= 0.1*final,1); high = find(v >= 0.9*final,1);
  assert(~isempty(low) && ~isempty(high));
  metrics = [t(high)-t(low),max(0,(max(v)-final)/final)*100,1-v(end)];
end
