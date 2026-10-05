%% Tablolar, tarihler ve veri analizi
% datatypes paketi ile table / string / datetime veri türleri.
clear; close all;
pkg load datatypes;
ad = string({'Deney A'; 'Deney B'; 'Deney C'; 'Deney D'});
gerilim = [3.7; 3.9; 3.6; 4.1];
akim = [1.2; 1.5; 1.1; 1.8];
guc = gerilim .* akim;
sonuclar = table(ad, gerilim, akim, guc, 'VariableNames', {'Deney','Gerilim','Akim','Guc'});
disp(sonuclar);
zaman = datetime('now');
disp(zaman);
figure('Name', 'Deney sonuçları');
bar(guc); xlabel('Deney'); ylabel('Güç (W)'); title('Ölçüm karşılaştırması'); grid on;
