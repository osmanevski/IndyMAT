% Grafik ve görselleştirme
%% grafik-cizgi-nesnesi | plot figure gca get set
figure();
h = plot(1:3, [2 4 6], 'r--', 'LineWidth', 2);
assert(isequal(get(h, 'YData'), [2 4 6]) && strcmp(get(h, 'Type'), 'line'));
assert(strcmp(get(h, 'LineStyle'), '--') && isequal(get(h, 'Color'), [1 0 0]) && get(h, 'LineWidth') == 2);
set(h, 'Marker', 'o');
assert(strcmp(get(h, 'Marker'), 'o'));
assert(strcmp(get(gca, 'Type'), 'axes'));
%% grafik-nokta-gosterimi | :grafik-nokta-gosterimi | grafik nesnelerinde h.Ozellik erişimi
h = plot(1:3);
h.LineWidth = 3;
assert(h.LineWidth == 3);
ax = gca;
ax.XLim = [0 5];
assert(isequal(xlim(), [0 5]));
%% grafik-coklu-cizgi-hold | plot hold findobj
h = plot(1:3, [1 2 3; 4 5 6]');
assert(numel(h) == 2);
hold on; plot(1:3, [7 8 9]); hold off;
assert(numel(findobj(gca, 'Type', 'line')) == 3);
%% grafik-etiketler | xlabel ylabel title legend grid text
plot(1:3); xlabel('x ekseni'); ylabel('y'); title('Baslik');
assert(strcmp(get(get(gca, 'XLabel'), 'String'), 'x ekseni'));
assert(strcmp(get(get(gca, 'Title'), 'String'), 'Baslik'));
l = legend('veri');
assert(isequal(get(l, 'String'), {'veri'}));
grid on;
assert(strcmp(get(gca, 'XGrid'), 'on'));
t = text(1, 2, 'not');
assert(strcmp(get(t, 'String'), 'not'));
%% grafik-eksen-sinirlari | xlim ylim axis
plot(1:10);
xlim([2 5]); ylim([0 20]);
assert(isequal(xlim(), [2 5]) && isequal(ylim(), [0 20]));
axis([0 1 0 2]);
assert(isequal(axis(), [0 1 0 2]));
%% grafik-subplot | subplot
a = subplot(2, 1, 1); b = subplot(2, 1, 2);
assert(a ~= b && numel(findobj(gcf, 'Type', 'axes')) == 2);
p1 = get(a, 'Position'); p2 = get(b, 'Position');
assert(p1(2) > p2(2));
%% grafik-tiledlayout | tiledlayout nexttile
tiledlayout(2, 1);
a = nexttile; plot(1:3);
b = nexttile; plot(3:-1:1);
assert(a ~= b);
%% grafik-temel-turler | bar barh stairs stem area errorbar scatter pie
assert(~isempty(bar([1 2 3])));
assert(~isempty(barh([1 2 3])));
assert(~isempty(stairs([1 2 3])));
assert(~isempty(stem([1 2 3])));
assert(~isempty(area([1 2 3])));
assert(~isempty(errorbar([1 2 3], [0.1 0.2 0.1])));
assert(~isempty(scatter([1 2 3], [3 2 1], 20, [1 0 0], 'filled')));
assert(~isempty(pie([1 2 3])));
%% grafik-logaritmik | semilogx semilogy loglog
semilogx([1 10 100], [1 2 3]);
assert(strcmp(get(gca, 'XScale'), 'log') && strcmp(get(gca, 'YScale'), 'linear'));
semilogy([1 2 3], [1 10 100]);
assert(strcmp(get(gca, 'YScale'), 'log'));
loglog([1 10], [1 100]);
assert(strcmp(get(gca, 'XScale'), 'log') && strcmp(get(gca, 'YScale'), 'log'));
%% grafik-uc-boyut | plot3 surf mesh contour meshgrid view
[X, Y] = meshgrid(-1:0.5:1);
Z = X.^2 + Y.^2;
h = surf(X, Y, Z);
assert(strcmp(get(h, 'Type'), 'surface') && isequal(get(h, 'ZData'), Z));
assert(~isempty(mesh(X, Y, Z)));
[c, hc] = contour(X, Y, Z);
assert(~isempty(c));
assert(strcmp(get(plot3([0 1], [0 1], [0 1]), 'Type'), 'line'));
view(30, 40);
[az, el] = view();
assert(az == 30 && el == 40);
%% grafik-fplot-cizim | fplot
fplot(@sin, [0 pi]);
assert(numel(get(gca, 'Children')) == 1);
%% grafik-fplot-nesne | fplot | çıktı olarak çizgi nesnesi
h = fplot(@sin, [0 pi]);
assert(~isempty(h));
%% grafik-polarplot | polarplot
h = polarplot(linspace(0, 2 * pi, 20), ones(1, 20));
assert(~isempty(h));
%% grafik-cift-y-ekseni | yyaxis
yyaxis left; plot(1:3);
yyaxis right; plot([10 20 30]);
%% grafik-sabit-cizgiler | xline yline
plot(1:5);
a = xline(2); b = yline(3);
assert(~isempty(a) && ~isempty(b));
%% grafik-parula | parula
assert(isequal(size(parula(16)), [16 3]));
%% grafik-renk-haritasi | colormap colorbar jet
m = colormap(jet(8));
assert(isequal(size(m), [8 3]));
imagesc(magic(4)); c = colorbar();
assert(~isempty(c));
%% grafik-histogram-nesnesi | histogram | Values ve BinEdges özellikleri
h = histogram([1 2 2 3 3 3], [0.5 1.5 2.5 3.5]);
assert(isequal(h.Values, [1 2 3]));
assert(isequal(h.BinEdges, [0.5 1.5 2.5 3.5]));
%% grafik-histogram-cizim | histogram
h = histogram([1 2 2 3 3 3], [0.5 1.5 2.5 3.5]);
assert(~isempty(h));
h = histogram(randn(1, 100), 10, 'Normalization', 'probability');
assert(~isempty(h));
%% grafik-ust-baslik | sgtitle
subplot(1, 2, 1); plot(1:3); subplot(1, 2, 2); plot(1:3);
sgtitle('Genel');
%% grafik-heatmap | heatmap
h = heatmap(magic(3));
assert(~isempty(h));
%% grafik-animatedline | animatedline addpoints
h = animatedline();
addpoints(h, 1, 2); addpoints(h, 2, 3);
[x, y] = getpoints(h);
assert(isequal(x, [1 2]) && isequal(y, [2 3]));
%% grafik-exportgraphics | exportgraphics
dosya = [tempname() '.png'];
plot(1:3);
exportgraphics(gcf, dosya);
bilgi = dir(dosya); delete(dosya);
assert(bilgi.bytes > 0);
%% grafik-patch-ve-doldurma | patch fill rectangle line
assert(strcmp(get(patch([0 1 1], [0 0 1], 'r'), 'Type'), 'patch'));
assert(~isempty(fill([0 1 1], [0 0 1], 'b')));
assert(~isempty(rectangle('Position', [0 0 1 1])));
assert(strcmp(get(line([0 1], [0 1]), 'Type'), 'line'));
%% grafik-renk-sirasi | colororder
plot(1:3);
colororder([1 0 0; 0 0 1]);
assert(isequal(get(gca, 'ColorOrder'), [1 0 0; 0 0 1]));
