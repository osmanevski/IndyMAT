%% Sinyal işleme — signal paketi gerekir
clear; close all;
try
  pkg load signal;
catch
  error('signal paketi bulunamadı. Projedeki paket kurulum rehberini izleyin.');
end
Fs=1000; t=0:1/Fs:1;
x=sin(2*pi*15*t)+0.5*sin(2*pi*180*t);
[b,a]=butter(5,50/(Fs/2));
y=filtfilt(b,a,x);
figure('Name','Alçak geçiren filtre');
plot(t,x,'Color',[.75 .78 .82]); hold on; plot(t,y,'LineWidth',1.5);
xlim([0 .3]); legend('Giriş','Filtrelenmiş'); grid on;
xlabel('Zaman (s)'); ylabel('Genlik'); title('Butterworth · 50 Hz');
