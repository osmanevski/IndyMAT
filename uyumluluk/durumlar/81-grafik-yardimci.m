% Grafik ve görselleştirme
%% grafiky-parula-boyut | parula | Parula uzunluğu sayısal tablo üretir
c = parula(17);
assert(isequal(size(c), [17 3]) && isa(c, 'double') && all(c(:) >= 0) && all(c(:) <= 1));
%% grafiky-parula-varsayilan | parula colormap | Varsayılan uzunluk geçerli şekil renk haritasından gelir
colormap(jet(9));
c = parula();
assert(isequal(size(c), [9 3]));
%% grafiky-renk-sirasi-axes | colororder plot get | Renk sırası eksen düzeyinde okunur ve yazılır
plot(1:3);
colors = [1 0 0; 0 0 1];
colororder(gca, colors);
assert(isequal(colororder(gca), colors) && isequal(get(gca, 'ColorOrder'), colors));
%% grafiky-renk-sirasi-figure | colororder | Figure hedefi renk sırasını saklar
f = gcf();
colors = [0.2 0.3 0.4; 0.8 0.7 0.6];
colororder(f, colors);
assert(isequal(colororder(f), colors));
%% grafiky-renk-sirasi-figure-axes | colororder axes | Figure renk sırası mevcut ve yeni eksenlere uygulanır
f = gcf(); a = axes('Parent', f);
colors = [0.2 0.3 0.4; 0.8 0.7 0.6];
colororder(f, colors);
assert(isequal(get(a, 'ColorOrder'), colors));
b = axes('Parent', f);
assert(isequal(get(b, 'ColorOrder'), colors));
%% grafiky-renk-sirasi-adlar | colororder | Renk adları ve kısa adlar RGB değerine çevrilir
colororder({'red', 'g', '#0000FF'});
assert(isequal(colororder(), [1 0 0; 0 1 0; 0 0 1]));
%% grafiky-tiledlayout-grid | tiledlayout nexttile plot | Izgara ayrı eksenleri doğru sırada üretir
t = tiledlayout(2, 2);
a = nexttile(); plot(1:3);
b = nexttile(); plot(3:-1:1);
assert(~isempty(t) && a ~= b && numel(findobj(gcf(), 'Type', 'axes')) == 2);
%% grafiky-tiledlayout-index | tiledlayout nexttile | Belirtilen tile dizini kullanılabilir
tiledlayout(2, 2);
a = nexttile(3);
assert(strcmp(get(a, 'Type'), 'axes'));
%% grafiky-tiledlayout-span | tiledlayout nexttile | Tile span eksen konumunu genişletir
tiledlayout(2, 2, 'TileSpacing', 'tight', 'Padding', 'loose');
a = nexttile([1 2]);
p = get(a, 'Position');
assert(p(3) > 0.5 && p(4) > 0);
%% grafiky-tiledlayout-spacing | tiledlayout nexttile | Boşluk seçenekleri geçerli eksen konumları üretir
tiledlayout(2, 1, 'TileSpacing', 'none', 'Padding', 'none');
a = nexttile(); b = nexttile();
pa = get(a, 'Position'); pb = get(b, 'Position');
assert(pa(2) > pb(2));
%% grafiky-yyaxis-left-right | yyaxis plot findobj | Sol ve sağ taraflara çizilen iki seri görünür
yyaxis left; plot(1:3, [1 2 3]);
yyaxis right; plot(1:3, [10 20 30]);
assert(numel(findobj(gcf(), 'Type', 'line')) == 2);
%% grafiky-yyaxis-target | yyaxis plot findobj | Açık eksen hedefi: MATLAB destekler, yardımcı açık hata verir
a = axes(); yyaxis(a, 'left'); plot(a, 1:3, 1:3);
yyaxis(a, 'right'); plot(1:3, [3 2 1]);
assert(numel(findobj(gcf(), 'Type', 'line')) == 2);
%% grafiky-polarplot-temel | polarplot get | Polar çizgi handle döndürür
h = polarplot(linspace(0, 2*pi, 30), ones(1, 30));
assert(~isempty(h) && strcmp(get(h, 'Type'), 'line'));
%% grafiky-polarplot-cizgi | polarplot get | Polar çizgi stili ve özellikleri uygulanır
h = polarplot([0 pi/2 pi], [1 2 1], 'r--', 'LineWidth', 2);
assert(strcmp(get(h, 'LineStyle'), '--') && get(h, 'LineWidth') == 2);
%% grafiky-exportgraphics-figure | exportgraphics dir delete plot | Figure PNG dışa aktarılır
f = figure(); plot(1:3);
file = [tempname() '.png'];
exportgraphics(f, file, 'Resolution', 96, 'BackgroundColor', 'white');
info = dir(file); delete(file);
assert(info.bytes > 0);
%% grafiky-exportgraphics-axes | exportgraphics dir delete | Eksen JPG dışa aktarılır
plot(1:3);
file = [tempname() '.jpg'];
exportgraphics(gca(), file, 'ContentType', 'image');
info = dir(file); delete(file);
assert(info.bytes > 0);
%% grafiky-exportgraphics-pdf | exportgraphics plot dir delete | Figure PDF dışa aktarılır
plot(1:3);
file = [tempname() '.pdf'];
exportgraphics(gcf(), file, 'ContentType', 'vector');
info = dir(file); delete(file);
assert(info.bytes > 0);
%% grafiky-parula-one | parula | Tek renk örneği üç sütun döndürür
c = parula(1);
assert(isequal(size(c), [1 3]));
%% grafiky-colororder-current | colororder plot | Varsayılan çağrı mevcut ekseni kullanır
plot(1:2);
colororder([0 1 0]);
assert(isequal(colororder(), [0 1 0]));
%% grafiky-nexttile-current | tiledlayout nexttile | Sırasız çağrılar sıradaki eksene ilerler
tiledlayout(1, 3);
a = nexttile(); b = nexttile(); c = nexttile();
assert(a ~= b && b ~= c && c ~= a);
%% grafiky-nexttile-reuse | tiledlayout nexttile | Aynı tile indeksi aynı ekseni döndürür
tiledlayout(1, 2);
a = nexttile(1); b = nexttile(1);
assert(a == b);
%% grafiky-nexttile-reselect | tiledlayout nexttile plot | Yeniden seçilen tile çizgisini korur ve geçerli eksen olur
tiledlayout(1, 2);
a = nexttile(1); h = plot([1 2 3]);
nexttile(2);
b = nexttile(1);
assert(a == b && gca() == a && isequal(get(h, 'YData'), [1 2 3]));
%% grafiky-yyaxis-limitler | yyaxis ylim plot | Etkin eksen ylim değerini tutar
yyaxis left; plot(1:3, [1 2 3]); ylim([0 4]);
assert(isequal(ylim(), [0 4]));
%% grafiky-polarplot-color | polarplot get | Renk adı polar çizgiye uygulanır
h = polarplot([0 pi], [1 1], 'Color', [0 1 0]);
assert(isequal(get(h, 'Color'), [0 1 0]));
%% grafiky-exportgraphics-tiff | exportgraphics plot dir delete | TIFF raster çıktısı üretir
plot(1:3);
file = [tempname() '.tif'];
exportgraphics(gcf(), file, 'Resolution', 72);
info = dir(file); delete(file);
assert(info.bytes > 0);
%% grafiky-parula-figure-size | parula colormap | Geçerli colormap uzunluğu parula varsayılanını belirler
colormap(jet(5));
assert(size(parula(), 1) == 5);
%% grafiky-yyaxis-ruler-after-plot | yyaxis plot | Çizim sağ cetveli korur ve aynı taraftaki önceki çizgiyi değiştirir
yyaxis left; plot(1:3);
yyaxis right; old = plot(10:10:30); plot(20:20:60);
assert(strcmp(get(gca(), 'YAxisLocation'), 'right') && ~isgraphics(old));
yyaxis left;
assert(strcmp(get(gca(), 'YAxisLocation'), 'left'));
%% grafiky-yyaxis-shared-hold | yyaxis plot hold | Hold iki tarafta da çizgileri korur
yyaxis left; a = plot(1:3); hold on;
yyaxis right; b = plot(10:10:30); c = plot(20:20:60);
assert(isgraphics(a) && isgraphics(b) && isgraphics(c));
hold off;
%% grafiky-yyaxis-cla-reset | yyaxis cla plot | cla reset ikinci cetveli kaldırır
yyaxis left; plot(1:3); yyaxis right; plot(10:10:30);
cla reset;
assert(strcmp(get(gca(), 'YAxisLocation'), 'left'));
assert(numel(findall(gcf(), 'Type', 'axes')) == 1);
%% grafiky-colororder-implicit-figure | colororder tiledlayout nexttile plot | Örtük renk sırası tüm şekle ve sonraki çizimlere uygulanır
tiledlayout(1, 2); a = nexttile(); b = nexttile();
colororder([1 0 0]);
assert(isequal(get(a, 'ColorOrder'), [1 0 0]) && isequal(get(b, 'ColorOrder'), [1 0 0]));
h = plot(1:3);
assert(isequal(get(h, 'Color'), [1 0 0]));
%% grafiky-tiledlayout-replaces-axes | tiledlayout nexttile plot | Yeni düzen eski normal ekseni kaldırır
plot(1:3); old = gca(); tiledlayout(1, 2); nexttile();
assert(~isgraphics(old) && numel(findall(gcf(), 'Type', 'axes')) == 1);
%% grafiky-nexttile-first-free | tiledlayout nexttile | Otomatik seçim ilk boş tile alanını bulur
tiledlayout(1, 2); b = nexttile(2); a = nexttile();
pa = get(a, 'Position'); pb = get(b, 'Position');
assert(pa(1) < pb(1));
%% grafiky-nexttile-deleted-slot | tiledlayout nexttile | Silinen önceki tile tekrar kullanılabilir
tiledlayout(1, 2); a = nexttile(); b = nexttile(); delete(a); c = nexttile();
pc = get(c, 'Position'); pb = get(b, 'Position');
assert(pc(1) < pb(1));
%% grafiky-nexttile-free-span | tiledlayout nexttile | Otomatik span seçimi boş bir dikdörtgen bulur
tiledlayout(2, 3); a = nexttile(1); b = nexttile([2 2]);
pa = get(a, 'Position'); pb = get(b, 'Position');
assert(pb(1) > pa(1) && pb(4) > pa(4));
%% grafiky-nexttile-obsolete-layout | tiledlayout nexttile | Eski düzen başvurusu yeni düzene yönlendirilmez
t1 = tiledlayout(1, 2); t2 = tiledlayout(2, 1);
failed = false;
try
    nexttile(t1);
catch
    failed = true;
end
assert(failed && ~isempty(t2));
%% grafiky-polarplot-property-first | polarplot | LineSpec olmadan özellik çiftleri kabul edilir
h = polarplot([0 pi], [1 1], 'LineWidth', 2);
assert(get(h, 'LineWidth') == 2);
h = polarplot([0 pi], [1 1], 'color', [1 0 0]);
assert(isequal(get(h, 'Color'), [1 0 0]));
h = polarplot([0 pi], [1 1], 'Marker', 'o');
assert(strcmp(get(h, 'Marker'), 'o'));
%% grafiky-exportgraphics-callback | exportgraphics plot | Dışa aktarma kaynak çizginin silme geri çağrısını çalıştırmaz
f = figure(); h = plot(1:3, 'DeleteFcn', @(src, evt)delete(f));
file = [tempname() '.png']; exportgraphics(f, file); delete(file);
assert(isgraphics(f) && isgraphics(h));
%% grafiky-exportgraphics-decorations | exportgraphics plot legend colorbar sgtitle | Başlık ve açıklamaları olan şekil dışa aktarılabilir
f = figure(); imagesc(magic(3)); hold on; h = plot(1:3);
lg = legend(h, 'series'); cb = colorbar(); sgtitle('Summary');
file = [tempname() '.png']; exportgraphics(f, file);
info = dir(file); delete(file);
assert(info.bytes > 0 && isgraphics(lg) && isgraphics(cb));
%% grafiky-exportgraphics-preserves-source | exportgraphics plot | Dışa aktarma kaynak geometrisini korur
f = figure('Units', 'pixels', 'Position', [100 100 720 240]); plot(1:3);
a = gca(); before = get(a, 'Position'); figbefore = get(f, 'Position');
file = [tempname() '.png']; exportgraphics(a, file); delete(file);
assert(isequal(get(a, 'Position'), before) && isequal(get(f, 'Position'), figbefore));
%% grafiky-colororder-invalid-hex | colororder | Geçersiz onaltılık renk kabul edilmez
failed = false;
try
    colororder({'#11223x'});
catch
    failed = true;
end
assert(failed);
%% grafiky-colororder-recolor-cycle | colororder plot | Mevcut otomatik çizgiler yeni paleti döngüsel kullanır
old = [1 0 0; 0 0 1]; new = [0 1 0; 1 0 1; 0 1 1];
colororder(old);
h = plot([1 2 3 4 5; 2 3 4 5 6]);
colororder(gca(), new);
for k = 1:5
    assert(isequal(get(h(k), 'Color'), new(mod(k - 1, 3) + 1, :)));
end
%% grafiky-colororder-explicit-color | colororder plot | Açıkça seçilen farklı bir renk palet değişirken korunur
chosen = [0.13 0.29 0.41];
h = plot(1:3, 'Color', chosen);
colororder([1 0 0; 0 1 0]);
assert(isequal(get(h, 'Color'), chosen));
assert(isequal(get(gca(), 'ColorOrder'), [1 0 0; 0 1 0]));
