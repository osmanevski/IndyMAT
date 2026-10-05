clc; clear; close all;

N = 128;
n = 0:N-1;

x = { rand(1, N), ...
      rand(1, N) + rand(1, N), ...
      sum(rand(12, N), 1) };

baslik = {'(a)  X = RND', '(b)  X = RND + RND', '(c)  X = RND + RND + ... + RND (12 kez)'};
ort    = [0.5, 1, 6];
sigma  = [1/sqrt(12), 1/sqrt(6), 1];

yy = linspace(0, 12, 1000);
pdfT = { double(yy >= 0 & yy <= 1), ...
         max(0, 1 - abs(yy - 1)), ...
         exp(-(yy - 6).^2 / 2) / sqrt(2*pi) };

figure('Color', 'w', 'Position', [100 50 1000 800]);

for k = 1:3
    axes('Position', [0.10, 0.72 - 0.30*(k-1), 0.62, 0.20], 'FontSize', 9);
    plot(n, x{k}, 'k.-', 'MarkerSize', 10);
    axis([0 N-1 0 12]);
    xticks(0:16:112); yticks(0:12);
    grid on;
    if k == 3, xlabel('Örnek numarası'); end
    ylabel('Genlik');
    title(baslik{k}, 'FontSize', 10);
    text(3, 11, sprintf(['teorik: ortalama = %.2f,  \\sigma = %.3f\n' ...
                         'ölçülen: ortalama = %.2f,  \\sigma = %.3f'], ...
                        ort(k), sigma(k), mean(x{k}), std(x{k})), ...
         'VerticalAlignment', 'top', 'BackgroundColor', 'w', 'EdgeColor', 'k', 'FontSize', 9);

    axes('Position', [0.79, 0.72 - 0.30*(k-1), 0.17, 0.20], 'FontSize', 9);
    if exist('histogram', 'file') || exist('histogram', 'builtin')
        histogram(x{k}, 'Normalization', 'pdf', 'Orientation', 'horizontal', ...
                  'FaceColor', [0.7 0.7 0.7]);
    else
        [adet, merkez] = hist(x{k}, 12);
        genislik = merkez(2) - merkez(1);
        barh(merkez, adet / (N * genislik), 1, 'FaceColor', [0.7 0.7 0.7]);
    end
    hold on;
    plot(pdfT{k}, yy, 'r', 'LineWidth', 1.5);
    hold off;
    ylim([0 12]); yticks(0:12);
    grid on;
    if k == 3, xlabel('pdf'); end
    title('pdf', 'FontSize', 10);
end

if exist('sgtitle', 'file') || exist('sgtitle', 'builtin')
    sgtitle('Rastgele değişkenlerin dağılımları', 'FontWeight', 'bold');
else
    axes('Position', [0 0.96 1 0.04], 'Visible', 'off');
    text(0.5, 0.5, 'Rastgele değişkenlerin dağılımları', 'HorizontalAlignment', 'center', ...
         'FontWeight', 'bold', 'FontSize', 12);
end
