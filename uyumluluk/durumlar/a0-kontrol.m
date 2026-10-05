% Kontrol sistemleri
%% kontrol-tf | tf pole zero dcgain tfdata
G = tf([1 3], [1 3 2]);
assert(norm(sort(pole(G)) - [-2; -1]) < 1e-10);
assert(abs(zero(G) + 3) < 1e-10);
assert(abs(dcgain(G) - 1.5) < 1e-12);
[pay, payda] = tfdata(G, 'v');
assert(isequal(pay(end-1:end), [1 3]) && isequal(payda, [1 3 2]));
%% kontrol-tf-s-degiskeni | tf
s = tf('s');
G = 1 / (s^2 + 3 * s + 2);
assert(abs(dcgain(G) - 0.5) < 1e-12);
%% kontrol-baglanti | feedback series parallel
G = tf(1, [1 1]);
assert(abs(dcgain(feedback(G, 1)) - 0.5) < 1e-12);
assert(abs(dcgain(series(G, tf(2, 1))) - 2) < 1e-12);
assert(abs(dcgain(parallel(G, G)) - 2) < 1e-12);
assert(abs(dcgain(G * G + 1) - 2) < 1e-12);
%% kontrol-zaman-yaniti | step impulse lsim
G = tf(1, [1 1]);
[y, t] = step(G, 0:0.1:8);
assert(abs(y(end) - (1 - exp(-8))) < 1e-4);
[y, t] = impulse(G, 0:0.1:2);
assert(abs(y(end) - exp(-2)) < 1e-3);
t = (0:0.01:5)';
y = lsim(G, ones(size(t)), t);
assert(abs(y(end) - (1 - exp(-5))) < 1e-3);
%% kontrol-stepinfo | stepinfo
S = stepinfo(tf(1, [1 1]));
assert(abs(S.RiseTime - log(9)) < 0.05);
assert(abs(S.Overshoot) < 1e-6);
%% kontrol-frekans-yaniti | bode freqresp
[genlik, faz] = bode(tf(1, [1 1]), 1);
assert(abs(genlik - 1 / sqrt(2)) < 1e-10 && abs(faz + 45) < 1e-8);
H = freqresp(tf(1, [1 1]), 1);
assert(abs(H - 1 / (1 + 1i)) < 1e-12);
%% kontrol-kararlilik-paylari | margin
[Gm, Pm, Wcg, Wcp] = margin(tf(1, [1 2 1 0]));
assert(abs(Gm - 2) < 1e-6 && abs(Wcg - 1) < 1e-6);
assert(Pm > 0 && Pm < 90);
%% kontrol-durum-uzayi | ss ssdata ctrb obsv
sistem = ss([0 1; -2 -3], [0; 1], [1 0], 0);
[A, B, C, D] = ssdata(sistem);
assert(isequal(A, [0 1; -2 -3]) && D == 0);
assert(rank(ctrb(A, B)) == 2 && rank(obsv(A, C)) == 2);
assert(abs(dcgain(sistem) - 0.5) < 1e-12);
assert(norm(sort(eig(sistem)) - [-2; -1]) < 1e-10);
%% kontrol-donusumler | ss tf zpk ss2tf tf2ss
G = tf(ss([0 1; -2 -3], [0; 1], [1 0], 0));
[pay, payda] = tfdata(G, 'v');
assert(norm(payda / payda(1) - [1 3 2]) < 1e-10);
Z = zpk([], [-1 -2], 2);
assert(abs(dcgain(Z) - 1) < 1e-12);
[b, a] = ss2tf([0 1; -2 -3], [0; 1], [1 0], 0);
assert(norm(a - [1 3 2]) < 1e-10 && abs(b(end) - 1) < 1e-10);
[A, B, C, D] = tf2ss(1, [1 3 2]);
assert(norm(sort(eig(A)) - [-2; -1]) < 1e-10 && numel(B) == 2 && numel(C) == 2 && D == 0);
%% kontrol-ayriklastirma | c2d
Gd = c2d(tf(1, [1 1]), 0.1);
assert(abs(pole(Gd) - exp(-0.1)) < 1e-10);
assert(abs(dcgain(Gd) - 1) < 1e-10);
Gt = c2d(tf(1, [1 1]), 0.1, 'tustin');
assert(abs(pole(Gt) - (1 - 0.05) / (1 + 0.05)) < 1e-10);
%% kontrol-tasarim | lqr place
A = [0 1; 0 0]; B = [0; 1];
K = lqr(A, B, eye(2), 1);
assert(norm(K - [1 sqrt(3)]) < 1e-8);
K = place(A, B, [-1 -2]);
assert(norm(sort(eig(A - B * K)) - [-2; -1]) < 1e-8);
%% kontrol-pid | pid
C = pid(2, 1);
T = feedback(C * tf(1, [1 1]), 1);
assert(abs(dcgain(T) - 1) < 1e-8);
%% kontrol-lyapunov-riccati | lyap care
X = lyap([-1 0; 0 -2], eye(2));
assert(norm(X - [0.5 0; 0 0.25]) < 1e-10);
X = care(-1, 1, 3, 1);
assert(abs(X - 1) < 1e-8);
%% kontrol-kok-yer-egrisi | rlocus
[r, k] = rlocus(tf(1, [1 3 2]));
assert(~isempty(r) && numel(k) == size(r, 2));
%% kontrol-sadelestirme | minreal
G = minreal(tf([1 1], [1 3 2]));
assert(abs(pole(G) + 2) < 1e-8);
%% kontrol-sonumleme | damp
[wn, zeta] = damp(tf(1, [1 2 4]));
assert(norm(wn - [2; 2]) < 1e-10 && norm(zeta - [0.5; 0.5]) < 1e-10);
%% kontrol-pade | pade
[pay, payda] = pade(1, 1);
assert(norm(pay / payda(1) - [-1 2]) < 1e-10 && norm(payda / payda(1) - [1 2]) < 1e-10);
%% kontrol-giris-gecikmesi | tf | InputDelay özelliği
G = tf(1, [1 1], 'InputDelay', 0.5);
assert(G.InputDelay == 0.5);
%% kontrol-cizimler | bode nyquist pzmap step
G = tf(1, [1 2 1]);
figure(); bode(G);
figure(); nyquist(G);
figure(); pzmap(G);
figure(); step(G);
