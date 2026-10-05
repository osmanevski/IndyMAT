%% IndyMAT'e hoş geldin
% Bu dosyayı Çalıştır ile çalıştır. Değişkenler sağda, grafikler altta.
clear; close all; clc;

%% Bir sinyali keşfet
Fs = 1000;                        % Örnekleme frekansı (Hz)
t = 0:1/Fs:1-1/Fs;
sinyal = sin(2*pi*5*t) + 0.35*sin(2*pi*20*t);

figure('Name', 'Sinyal laboratuvarı', 'Position', [0 0 1000 400]);
subplot(1,2,1);
plot(t, sinyal, 'Color', [0.1 0.4 0.8], 'LineWidth', 1.5);
xlabel('Zaman (s)'); ylabel('Genlik'); title('İki frekans, tek sinyal'); grid on;

%% Frekans uzayına geç
N = length(sinyal);
f = (0:N/2)*Fs/N;
spektrum = abs(fft(sinyal))/N;
subplot(1,2,2);
stem(f, 2*spektrum(1:N/2+1), 'Color', [0.9 0.4 0.15]);
xlim([0 40]); xlabel('Frekans (Hz)'); ylabel('Genlik'); title('FFT · 5 Hz + 20 Hz'); grid on;

fprintf('Örnek sayısı: %d\n', N);
fprintf('Sinyalin RMS değeri: %.4f\n', sqrt(mean(sinyal.^2)));
disp('Hazır. Komut penceresinde sinyal(1:10) yazmayı dene.');
