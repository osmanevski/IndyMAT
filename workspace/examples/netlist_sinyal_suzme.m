%% FIR ve IIR — açık Gain/Sum/UnitDelay fark denklemleri
% Önce sinyal_suzme.m dosyasını aynı oturumda çalıştırın.
% Özgün betiğin x ve katsayıları alınır; netlist RNG çalıştırmaz.
% Sıfır filtre ilk durumu ve nedensel gecikme korunur; filter bloğu yoktur.
if ~exist('x','var') || ~exist('Fs','var') || ~exist('b_fir','var') || ...
   ~exist('a_fir','var') || ~exist('b_iir','var') || ~exist('a_iir','var')
  error('Önce sinyal_suzme.m dosyasını aynı oturumda çalıştırın.');
end
N = numel(x);
parametreler = struct('values',x,'b_fir',b_fir,'a_fir',a_fir, ...
                     'b_iir',b_iir,'a_iir',a_iir,'Ts',1/Fs);
model = im_example_model('sinyal_suzme',parametreler);
netlist_sonucu = im_simulate(model);
y = netlist_sonucu.signals.output;
X = fft([x y]); spektrum = abs(X(1:N/2+1,:))/N;
spektrum(2:end-1,:) = 2*spektrum(2:end-1,:); f = (0:N/2)'*Fs/N;
%% Grafikler — açık kullanıcı komutları
figure('Name','Netlist: FIR ve IIR');
subplot(2,1,1); plot(netlist_sonucu.t,[netlist_sonucu.signals.input y]); grid on;
xlim([0 0.4]); xlabel('Zaman (s)'); ylabel('Genlik'); legend('Giriş','FIR','IIR');
subplot(2,1,2); plot(f,spektrum); grid on;
xlabel('Frekans (Hz)'); ylabel('Tek taraflı genlik'); legend('Giriş','FIR','IIR');
