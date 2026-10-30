//! Signal-processing builtins checked against values measured with Octave 11
//! `signal` 1.4.8 (behaviour oracle only). Relative tolerance 1e-9.

use indy_interp::Interp;

fn run(src: &str) -> Interp {
    let mut it = Interp::new();
    if let Err(e) = it.run_text(src) {
        panic!("{}\n  -> {} ({})", src, e.message, e.identifier);
    }
    it
}

fn check(src: &str, re: &[f64], im: &[f64]) {
    let it = run(src);
    let n = it.get_var("r").expect("no r").to_num().expect("numeric r");
    let gi = n.im.clone().unwrap_or_else(|| vec![0.0; n.re.len()]);
    assert!(
        n.re.len() >= re.len(),
        "{}: only {} values",
        src,
        n.re.len()
    );
    for (k, w) in re.iter().enumerate() {
        assert!(
            (n.re[k] - w).abs() <= 1e-9 * (1.0 + w.abs()),
            "{}: re[{}] = {}, want {}",
            src,
            k,
            n.re[k],
            w
        );
    }
    for (k, w) in im.iter().enumerate() {
        assert!(
            (gi[k] - w).abs() <= 1e-9 * (1.0 + w.abs()),
            "{}: im[{}] = {}, want {}",
            src,
            k,
            gi[k],
            w
        );
    }
}

#[test]
fn t_fir1_lp() {
    check(
        "r=fir1(20,0.3);",
        &[
            9.341761675e-19,
            0.002928903179,
            0.006342347322,
            0.003783041972,
            -0.01238784806,
            -0.03432657283,
        ],
        &[],
    );
}

#[test]
fn t_fir1_hp() {
    check(
        "r=fir1(20,0.3,\"high\");",
        &[
            -4.049811415e-18,
            -0.002930143801,
            -0.006345033808,
            -0.003784644389,
            0.01239309529,
            0.03434111285,
        ],
        &[],
    );
}

#[test]
fn t_fir1_hp_odd() {
    check(
        "r=fir1(21,0.3,\"high\");",
        &[
            0.001877320239,
            -5.010267165e-18,
            -0.00438890648,
            -0.009056673549,
            -0.004914507802,
            0.01483266746,
        ],
        &[],
    );
}

#[test]
fn t_fir1_bp() {
    check(
        "r=fir1(30,[0.2 0.5]);",
        &[
            -0.001689977781,
            -0.001198027478,
            0.0001428823357,
            -0.004215356787,
            -0.0106204621,
            8.420790301e-18,
        ],
        &[],
    );
}

#[test]
fn t_fir1_bs() {
    check(
        "r=fir1(30,[0.2 0.5],\"stop\");",
        &[
            0.001692046313,
            0.001199493863,
            -0.0001430572236,
            0.004220516382,
            0.01063346154,
            -2.047552212e-17,
        ],
        &[],
    );
}

#[test]
fn t_fir1_noscale() {
    check(
        "r=fir1(20,0.3,\"noscale\");",
        &[
            9.355612398e-19,
            0.002933245767,
            0.006351750912,
            0.003788650964,
            -0.01240621511,
            -0.03437746771,
        ],
        &[],
    );
}

#[test]
fn t_fir1_mb() {
    check(
        "r=fir1(40,[0.2 0.4 0.6],\"DC-1\");",
        &[
            -1.249581443e-18,
            -0.0008450140757,
            0.0004077160678,
            -0.002321463867,
            -0.008328990242,
            1.006213964e-17,
        ],
        &[],
    );
}

#[test]
fn t_fir1_mb0() {
    check(
        "r=fir1(40,[0.2 0.4 0.6]);",
        &[
            -1.861100689e-18,
            0.0008390309607,
            -0.0004048292377,
            0.002305026762,
            0.008270016898,
            1.443690326e-17,
        ],
        &[],
    );
}

#[test]
fn t_firls() {
    check(
        "r=firls(30,[0 0.3 0.4 1],[1 1 0 0]);",
        &[
            -0.004692883289,
            0.00163814821,
            0.009818117749,
            0.008279878771,
            -0.00626749116,
            -0.01958900144,
        ],
        &[],
    );
}

#[test]
fn t_bu_b() {
    check(
        "[r,q]=butter(4,0.2);",
        &[
            0.004824343358,
            0.01929737343,
            0.02894606015,
            0.01929737343,
            0.004824343358,
        ],
        &[],
    );
}

#[test]
fn t_buh_a() {
    check(
        "[q,r]=butter(3,0.3,\"high\");",
        &[1.0, -1.161917484, 0.6959427558, -0.1377613013],
        &[],
    );
}

#[test]
fn t_bubp_b() {
    check(
        "[r,q]=butter(3,[0.2 0.4]);",
        &[0.01809893301, 0.0, -0.05429679902, 0.0, 0.05429679902, 0.0],
        &[],
    );
}

#[test]
fn t_bubs_b() {
    check(
        "[r,q]=butter(3,[0.2 0.4],\"stop\");",
        &[
            0.5276243825,
            -1.95653881,
            4.001288117,
            -4.909519387,
            4.001288117,
            -1.95653881,
        ],
        &[],
    );
}

#[test]
fn t_c1_b() {
    check(
        "[r,q]=cheby1(4,1,0.3);",
        &[
            0.008363239556,
            0.03345295822,
            0.05017943733,
            0.03345295822,
            0.008363239556,
        ],
        &[],
    );
}

#[test]
fn t_c1bp_a() {
    check(
        "[q,r]=cheby1(3,0.5,[0.2 0.5]);",
        &[
            1.0,
            -2.235152566,
            3.15931502,
            -2.983937042,
            2.204778973,
            -1.014343785,
        ],
        &[],
    );
}

#[test]
fn t_c2_b() {
    check(
        "[r,q]=cheby2(4,40,0.3);",
        &[
            0.01826742402,
            -0.009311100531,
            0.02566926612,
            -0.009311100531,
            0.01826742402,
        ],
        &[],
    );
}

#[test]
fn t_c2h_b() {
    check(
        "[r,q]=cheby2(5,30,0.3,\"high\");",
        &[
            0.2511857102,
            -0.9821553619,
            1.75344729,
            -1.75344729,
            0.9821553619,
            -0.2511857102,
        ],
        &[],
    );
}

#[test]
fn t_c25_b() {
    check(
        "[r,q]=cheby2(5,30,0.3);",
        &[
            0.04831558378,
            -0.01892900933,
            0.04458052192,
            0.04458052192,
            -0.01892900933,
            0.04831558378,
        ],
        &[],
    );
}

#[test]
fn t_bus_a() {
    check(
        "[q,r]=butter(3,100,\"s\");",
        &[1.0, 200.0, 20000.0, 1000000.0],
        &[],
    );
}

#[test]
fn t_fz_h() {
    check(
        "r=freqz([1 2 3],[1 -0.5],8);",
        &[
            12.0,
            6.504713917,
            -2.457084395e-16,
            -2.445843217,
            -2.4,
            -1.299284541,
        ],
        &[
            0.0,
            -7.678155852,
            -6.828427125,
            -3.511059271,
            -0.8,
            0.7334188535,
        ],
    );
}

#[test]
fn t_fzf_f() {
    check(
        "[q,r]=freqz([1 2 3],[1 -0.5],6,1000);",
        &[
            0.0,
            83.33333333,
            166.6666667,
            250.0,
            333.3333333,
            416.6666667,
        ],
        &[],
    );
}

#[test]
fn t_gd() {
    check(
        "r=grpdelay([1 2 3],[1 -0.5],6);",
        &[
            2.333333333,
            1.848157788,
            1.526315789,
            1.8,
            2.714285714,
            2.120121356,
        ],
        &[],
    );
}

#[test]
fn t_impz() {
    check(
        "r=impz([1 2],[1 -0.5],8);",
        &[1.0, 2.5, 1.25, 0.625, 0.3125, 0.15625],
        &[],
    );
}

#[test]
fn t_ff() {
    check(
        "x = sin(0.2*(1:60)) + cos(0.9*(1:60)); [b,a] = butter(3,0.2); r=filtfilt(b,a,x);",
        &[
            0.8343968406,
            0.6314537506,
            0.5213998797,
            0.553036004,
            0.7012168441,
            0.884052226,
        ],
        &[],
    );
}

#[test]
fn t_hilb() {
    check(
        "x = sin(0.2*(1:40)) + cos(0.9*(1:40)).^2; r=hilbert(x);",
        &[
            0.5850682834,
            0.4410391341,
            1.381988911,
            1.521531748,
            0.8859058539,
        ],
        &[
            0.2202817079,
            -0.5178851754,
            -0.7652895301,
            0.1060114637,
            -0.0161230923,
        ],
    );
}

#[test]
fn t_pg() {
    check(
        "x = sin(0.2*(1:40)) + cos(0.9*(1:40)).^2; r=periodogram(x);",
        &[
            2.640623003,
            4.619406185,
            2.990543518,
            1.263409587,
            0.3339467855,
        ],
        &[],
    );
}

#[test]
fn t_pw() {
    check(
        "x = sin(0.2*(1:40)) + cos(0.9*(1:40)).^2; r=pwelch(x,16,8,64,100);",
        &[
            0.06758691486,
            0.1281545026,
            0.1088335724,
            0.08195917216,
            0.05384674442,
            0.03017840713,
        ],
        &[],
    );
}

#[test]
fn t_xc() {
    check(
        "x = sin(0.2*(1:40)) + cos(0.9*(1:40)).^2; r=xcorr(x(1:8));",
        &[
            0.8013475065,
            1.765534095,
            3.549390601,
            5.934514629,
            7.359589194,
            8.321979729,
        ],
        &[],
    );
}

#[test]
fn t_xcc() {
    check(
        "x = sin(0.2*(1:40)) + cos(0.9*(1:40)).^2; r=xcorr(x(1:6),\"coeff\");",
        &[
            0.1065693529,
            0.151060728,
            0.4265135171,
            0.6461055244,
            0.7506041938,
            1.0,
        ],
        &[],
    );
}

#[test]
fn t_xcu() {
    check(
        "x = sin(0.2*(1:40)) + cos(0.9*(1:40)).^2; r=xcorr(x(1:6),2,\"unbiased\");",
        &[
            1.183744591,
            1.100159179,
            1.221415126,
            1.100159179,
            1.183744591,
        ],
        &[],
    );
}

// ---------------------------------------------------------------------------
// Property checks (no oracle exists for these in the available Octave build)
// ---------------------------------------------------------------------------

fn truth(src: &str) {
    let it = run(src);
    let v = it.get_var("ok").expect("no ok");
    assert!(v.is_true().unwrap(), "{}", src);
}

#[test]
fn p_ellip_meets_spec() {
    truth(
        "[b,a]=ellip(4,1,40,0.3); h=abs(freqz(b,a,[0.01 0.15 0.299 0.301 0.6 0.9]*pi)); \
         ok = abs(20*log10(h(2)))<1.0001 && abs(20*log10(h(3)))<1.0001 && 20*log10(h(5))<-39.999 && 20*log10(h(6))<-39.999;",
    );
    truth(
        "[b,a]=ellip(5,0.5,50,[0.2 0.5]); h=abs(freqz(b,a,[0.35 0.1 0.8]*pi)); \
         ok = abs(20*log10(h(1)))<0.5001 && 20*log10(h(2))<-49.99 && 20*log10(h(3))<-49.99;",
    );
    truth("[z,p,k]=ellip(3,1,40,0.3); ok = numel(z)==3 && numel(p)==3 && all(abs(p)<1);");
}

#[test]
fn p_ellip_analog_prototype_dc_gain() {
    truth("[b,a]=ellip(4,1,40,100,'s'); ok = abs(abs(b(end)/a(end)) - 10^(-1/20)) < 1e-9;");
    truth("[b,a]=ellip(3,1,40,100,'s'); ok = abs(abs(b(end)/a(end)) - 1) < 1e-9;");
}

#[test]
fn p_state_space_outputs() {
    truth(
        "[b,a]=butter(3,0.3); [A,B,C,D]=butter(3,0.3); \
         [bb,aa]=ss2tf_check(A,B,C,D); ok = max(abs(bb-b))<1e-12 && max(abs(aa-a))<1e-12; \
         function [bb,aa]=ss2tf_check(A,B,C,D), aa=poly(A); bb=poly(A-B*C)+(D-1)*poly(A); end",
    );
}

#[test]
fn p_fir2_firls_response() {
    truth(
        "b=fir2(60,[0 0.3 0.5 1],[1 1 0 0]); h=abs(freqz(b,1,[0.1 0.25 0.7 0.9]*pi)); \
         ok = abs(h(1)-1)<0.02 && abs(h(2)-1)<0.05 && h(3)<0.02 && h(4)<0.02 && numel(b)==61;",
    );
    truth(
        "b=firls(41,[0 0.3 0.4 1],[1 1 0 0]); h=abs(freqz(b,1,[0.1 0.25 0.6 0.9]*pi)); \
         ok = numel(b)==42 && abs(h(1)-1)<0.02 && h(3)<0.05 && max(abs(b-fliplr(b)))<1e-12;",
    );
}

#[test]
fn p_filtfilt_zero_phase_and_sos() {
    truth(
        "[b,a]=butter(4,0.2); x=sin(0.05*pi*(1:200)'); y=filtfilt(b,a,x); \
         ok = max(abs(y(40:160)-x(40:160)*abs(freqz(b,a,[0.05 0.05]*pi)(1))^2))<5e-3 && size(y,1)==200;",
    );
    truth(
        "[z,p,k]=butter(5,0.3); [sos,g]=zp2sos(z,p,k); [b,a]=butter(5,0.3); [bb,aa]=sos2tf(sos,g); \
         x=randn(1,64); ok = max(abs(bb-b))<1e-12 && max(abs(aa-a))<1e-12 && max(abs(sosfilt(sos,x)*g-filter(b,a,x)))<1e-10;",
    );
    truth(
        "[b,a]=butter(3,0.2); [sos,g]=tf2sos(b,a); [bb,aa]=sos2tf(sos,g); ok = max(abs(bb-b))<1e-12 && max(abs(aa-a))<1e-12;",
    );
}

#[test]
fn p_zero_pole_conversions() {
    truth(
        "[z,p,k]=tf2zp([2 4 6],[1 0.5 0.06]); [b,a]=zp2tf(z,p,k); ok = max(abs(b-[2 4 6]))<1e-12 && max(abs(a-[1 0.5 0.06]))<1e-12 && k==2;",
    );
    truth(
        "[b,a]=zp2tf([0.5;-0.3],[0.1+0.2i;0.1-0.2i],2); ok = isreal(b) && isreal(a) && abs(b(2)-2*(-0.2))<1e-12;",
    );
}

#[test]
fn p_resample_decimate_interp() {
    truth(
        "n=0:199; x=sin(2*pi*0.02*n); y=resample(x,3,2); m=0:numel(y)-1; ref=sin(2*pi*0.02*m*2/3); \
         ok = numel(y)==300 && max(abs(y(40:260)-ref(40:260)))<5e-3;",
    );
    truth(
        "n=0:199; x=sin(2*pi*0.01*n); y=resample(x,1,2); ref=sin(2*pi*0.01*2*(0:numel(y)-1)); \
         ok = numel(y)==100 && max(abs(y(20:80)-ref(20:80)))<5e-3;",
    );
    truth("y=decimate(ones(1,200),4); ok = numel(y)==50 && abs(y(25)-1)<2e-2;");
}

#[test]
fn p_small_utilities() {
    truth(
        "ok = isequal(upsample([1 2 3],3), [1 0 0 2 0 0 3 0 0]) && isequal(downsample(1:10,3,1), [2 5 8]);",
    );
    truth(
        "ok = isequal(medfilt1([1 5 2 8 3 9 4],3), [1 2 5 3 8 4 4]) && isequal(medfilt1([1 5 2 8],3,'truncate'), [3 2 5 5]);",
    );
    truth(
        "[p,l]=findpeaks([1 3 2 5 4 6 1 2 0],'SortStr','descend','NPeaks',2); ok = isequal(p,[6 5]) && isequal(l,[6 4]);",
    );
    truth(
        "[p,l,w,pr]=findpeaks([0 4 0 2 0 6 0]); ok = isequal(p,[4 2 6]) && isequal(pr,[4 2 6]) && all(abs(w-1)<1e-12);",
    );
    truth("[p,l]=findpeaks([0 4 0 2 0 6 0],'MinPeakDistance',3); ok = isequal(l,[2 6]);");
    truth("x=randn(1,50); ok = max(abs(fftfilt([1 2 3],x)-filter([1 2 3],1,x)))<1e-12;");
}

#[test]
fn p_spectral_identities() {
    truth(
        "x=randn(1,64); X=fft(x); [P,w]=periodogram(x,ones(1,64),64); \
         ok = abs(sum(P)*(w(2)-w(1)) - mean(x.^2)) < 1e-12 && numel(w)==33 && abs(w(end)-pi)<1e-12;",
    );
    truth(
        "x=sin(2*pi*0.125*(0:255)); y=hilbert(x); ok = max(abs(imag(y(20:230)) + cos(2*pi*0.125*(19:229))))<1e-10 && max(abs(real(y)-x))<1e-12;",
    );
    truth(
        "fs=100; x=sin(2*pi*10*(0:1023)/fs); [P,f]=pwelch(x,256,128,256,fs); [m,i]=max(P); ok = abs(f(i)-10)<=fs/256;",
    );
    truth(
        "x=randn(1,100); [c,l]=xcorr(x,'coeff'); ok = abs(c(100)-1)<1e-12 && isequal(l,-99:99) && max(abs(c-fliplr(c)))<1e-12;",
    );
    truth(
        "[S,F,T,P]=spectrogram(randn(1,128),16,8,32,100); ok = size(S)==[17 15] && numel(F)==17 && numel(T)==15 && T(1)==8/100;",
    );
    truth("x=[1 2 3 2 1]; e=envelope(x); ok = numel(e)==5 && all(e>=x-1e-12);");
}

#[test]
fn p_group_delay_and_impz() {
    truth("g=grpdelay(fir1(10,0.3),1,8); ok = max(abs(g-5))<1e-9;");
    truth("h=impz([1],[1 -0.5],5); ok = max(abs(h(:)'-0.5.^(0:4)))<1e-14;");
    truth("h=impz(1,[1 -0.5]); ok = numel(h)>=10 && abs(h(1)-1)<1e-15;");
    truth("[h,w]=freqz([1 1],1,4,'whole'); ok = isequal(size(h),[4 1]) && abs(w(4)-1.5*pi)<1e-12;");
    truth("[h,f]=freqz([1 1],1,4,200); ok = abs(f(2)-25)<1e-12;");
}

#[test]
fn p_orders() {
    truth("[n,wn]=buttord(0.2,0.3,1,40); ok = n==11 || n==10 || n==12;");
    truth(
        "[n,wn]=cheb1ord(0.2,0.3,1,40); [b,a]=cheby1(n,1,wn); h=abs(freqz(b,a,[0.2 0.3]*pi)); ok = 20*log10(h(1))>-1.0001 && 20*log10(h(2))<-39.9;",
    );
}

#[test]
fn p_errors_and_pkg() {
    let mut it = Interp::new();
    it.run_text("pkg load signal").expect("pkg load signal");
    for (src, frag) in [
        ("fir1(20,1.2)", "cutoff"),
        ("butter(0,0.2)", "order"),
        ("filtfilt([1 1],1,[1 2 3])", "3 times"),
        ("freqz()", "Not enough"),
        ("butter(3,[0.4 0.2])", "increasing"),
        ("pkg load nosuchpackage", "package"),
    ] {
        let mut it = Interp::new();
        let e = it.run_text(src).expect_err(src);
        assert!(e.message.contains(frag), "{}: {}", src, e.message);
    }
}

#[test]
fn p_interp_waveforms_and_db() {
    truth(
        "n=0:63; x=sin(2*pi*0.03*n); y=interp(x,4); m=0:numel(y)-1; ref=sin(2*pi*0.03*m/4); \
         ok = numel(y)==256 && max(abs(y(40:200)-ref(40:200)))<2e-2 && max(abs(y(1:4:end-8)(10:50)-x(10:50)))<2e-2;",
    );
    truth("ok = isa(interp(single(1:9),2),'single') && isequal(interp(1:9,1),1:9);");
    truth(
        "ok = isequal(square([0 pi/2 3*pi/2]),[1 1 -1]) && abs(sawtooth(pi))<1e-12 && abs(rms([3 4])-sqrt(12.5))<1e-12;",
    );
    truth(
        "ok = abs(db2mag(20)-10)<1e-12 && abs(mag2db(10)-20)<1e-12 && abs(pow2db(100)-20)<1e-12 && abs(db2pow(20)-100)<1e-12;",
    );
    truth(
        "c=chirp(0:0.01:1,0,1,10); ok = numel(c)==101 && abs(c(1)-1)<1e-15 && abs(c(end)-cos(2*pi*5))<1e-12;",
    );
    truth(
        "[b,a]=sos2tf([1 2 3 1 -.5 .1;1 0 0 1 -.2 .05]); ok = isequal(b,[1 2 3 0]) && max(abs(a-[1 -.7 .25 -.045 .005]))<1e-13;",
    );
}
