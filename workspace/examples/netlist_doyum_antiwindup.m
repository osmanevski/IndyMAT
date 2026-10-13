%% PI doyumu ve clamping — Gain/Sum/UnitDelay ile Euler integrali
% İki PI aynı anda güncellenir: korumasız ve ClampingGate korumalı.
% Tesis ZOH ile tam, integral ileri Euler; h ve h/2 sonuçları saklanır.
hler = [0.002 0.001]; netlist_sonuclari = cell(1,2);
for q = 1:2
  model = im_example_model('doyum_antiwindup',struct('Ts',hler(q)));
  netlist_sonuclari{q} = im_simulate(model);
end
netlist_sonucu = netlist_sonuclari{1};
t = netlist_sonucu.t; y = netlist_sonucu.signals.output;
u = netlist_sonucu.signals.actuator;
%% Grafikler — açık kullanıcı komutları
figure('Name','Netlist: doyum ve anti-windup');
subplot(2,1,1); plot(t,y); grid on; ylabel('Çıkış');
legend('Korumasız PI','Clamping PI');
subplot(2,1,2); plot(t,u); grid on; xlabel('Zaman (s)'); ylabel('Aktüatör');
