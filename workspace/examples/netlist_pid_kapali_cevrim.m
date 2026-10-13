%% P, PI ve filtreli PID — denklem netlistleri
% im_example_model temel blokları kurar; im_simulate gerçek Octave ode45 kullanır.
% Bu dar netlist katmanı Simulink model dosyası veya uyumluluğu değildir.
kazanclar = [2 0 0; 2 1 0; 2 1 1]; adlar = {'P','PI','PID'};
netlist_sonuclari = cell(1,3);
for j = 1:3
  model = im_example_model('pid_kapali_cevrim',struct('gains',kazanclar(j,:)));
  netlist_sonuclari{j} = im_simulate(model);
end
t = netlist_sonuclari{1}.t;
y = [netlist_sonuclari{1}.signals.output, netlist_sonuclari{2}.signals.output, ...
     netlist_sonuclari{3}.signals.output];
%% Grafikler — açık kullanıcı komutları
figure('Name','Netlist: P, PI ve PID'); plot(t,y); grid on;
xlabel('Zaman (s)'); ylabel('Çıkış'); legend(adlar); xlim([0 15]);
