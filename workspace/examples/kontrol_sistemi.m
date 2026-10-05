%% Kontrol sistemleri — control paketi gerekir
clear; close all;
try
  pkg load control;
catch
  error('control paketi bulunamadı. Projedeki paket kurulum rehberini izleyin.');
end
s = tf('s');
G = 10/(s^2+2*s+10);
figure('Name','Basamak yanıtı'); step(G); grid on;
figure('Name','Bode diyagramı'); bode(G); grid on;
disp('Sistem kutupları:'); disp(pole(G));
