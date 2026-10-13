%% Kütle-yay-sönüm — iki Integrator ile denklem netlisti
% m=1, c=0.8, k=4; x(0)=1, v(0)=0, dış kuvvet sıfır.
model = im_example_model('kutle_yay_sonum');
netlist_sonucu = im_simulate(model);
t = netlist_sonucu.t; y = netlist_sonucu.signals.output;
%% Grafikler — açık kullanıcı komutları
figure('Name','Netlist: kütle-yay-sönüm'); plot(t,y); grid on;
xlabel('Zaman (s)'); ylabel('Konum (m)');
