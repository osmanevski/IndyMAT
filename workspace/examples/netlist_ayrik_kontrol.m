%% Ayrık kontrol — tam ZOH tesis güncellemesi ve UnitDelay
% K=2; x[k+1]=exp(-Ts)*x[k]+(1-exp(-Ts))*K*(1-x[k]).
Tsler = [0.05 0.5]; netlist_sonuclari = cell(1,2);
for j = 1:2
  model = im_example_model('ayrik_kontrol',struct('Ts',Tsler(j)));
  netlist_sonuclari{j} = im_simulate(model);
end
%% Grafikler — açık kullanıcı komutları
figure('Name','Netlist: ayrık kontrol');
stairs(netlist_sonuclari{1}.t,netlist_sonuclari{1}.signals.output); hold on;
stairs(netlist_sonuclari{2}.t,netlist_sonuclari{2}.signals.output); grid on;
xlabel('Zaman (s)'); ylabel('Çıkış'); legend('Ts = 0.05 s','Ts = 0.5 s');
