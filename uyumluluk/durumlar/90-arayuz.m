% Uygulama arayüzü
%% arayuz-uicontrol | uicontrol
f = figure('Visible', 'off');
b = uicontrol(f, 'Style', 'pushbutton', 'String', 'Tamam');
assert(strcmp(get(b, 'String'), 'Tamam') && strcmp(get(b, 'Style'), 'pushbutton'));
%% arayuz-uifigure | uifigure uibutton uilabel
f = uifigure('Visible', 'off');
b = uibutton(f, 'Text', 'Tamam');
l = uilabel(f, 'Text', 'Durum');
assert(strcmp(b.Text, 'Tamam') && strcmp(l.Text, 'Durum'));
delete(f);
%% arayuz-guidata | guidata
f = figure('Visible', 'off');
guidata(f, struct('sayac', 3));
veri = guidata(f);
assert(veri.sayac == 3);
