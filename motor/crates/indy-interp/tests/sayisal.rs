//! Phase 3 numerics: interpolation, polynomials, windows, statistics,
//! complex linear algebra, bit functions, periodogram. Each test is a
//! MATLAB snippet with `assert`; reference values were checked with
//! `octave-cli` (see `octave_xc/` and `scripts/octave_xc.sh`) or are
//! analytic.

use indy_interp::Interp;

fn run(src: &str) {
    let mut it = Interp::new();
    if let Err(e) = it.run_text(src) {
        panic!("{}\n  -> {} ({})", src, e.message, e.identifier);
    }
}

#[test]
fn interp1_methods() {
    run(r#"x = [1 2 4 7 8]; y = [3 1 4 1 5]; q = [1.5 3 4.5 7.9];
        assert(norm(interp1(x,y,q) - [2 2.5 3.5 4.6]) < 1e-12);
        assert(all(isnan(interp1(x,y,[0 9]))));
        assert(isequal(interp1(x,y,[0 9],'linear',-7), [-7 -7]));
        v = interp1(x,y,[0 9],'linear','extrap'); assert(abs(v(1) - 5) < 1e-12 && abs(v(2) - 9) < 1e-12);
        assert(isequal(interp1(x,y,[1.4 1.6 3 5.5],'nearest'), [3 1 4 1]));
        assert(isequal(interp1(x,y,[1.5 3 7.5],'previous'), [3 1 1]));
        assert(isequal(interp1(x,y,[1.5 3 7.5],'next'), [1 4 5]));
        v = interp1(x,y,[1.5 3 4.5 7.9],'pchip'); assert(norm(v - [1.60416667 2.5 3.77777778 4.46275]) < 1e-6);
        v = interp1(x,y,[1.5 3 4.5],'spline'); assert(norm(v - [1.3463031 2.49183007 3.79973448]) < 1e-7);
        v = interp1(x,y,[0 9],'spline'); assert(abs(v(1) - 11.9673203) < 1e-6);
        v = interp1(x,y,[0 9],'spline',0); assert(isequal(v,[0 0]));
        assert(isequal(interp1([4 1 2],[10 20 30],[1.5 3]), [25 20]));
        Y = [y; y.^2]'; v = interp1(x,Y,[1.5 3 5]); assert(isequal(size(v),[3 2]));
        assert(norm(v(:,2)' - [interp1(x,y.^2,1.5) interp1(x,y.^2,3) interp1(x,y.^2,5)]) < 1e-12);
        assert(isequal(size(interp1(x,y,[1.5 3; 5 6])), [2 2]));
        assert(interp1([1 2 3],[10 20 30],2.5) == 25);
        assert(isequal(interp1([10 20 30], 2.5), 25));"#);
}

#[test]
fn spline_pchip_makima() {
    run(r#"x = [1 2 4 7 8]; y = [3 1 4 1 5];
        pp = spline(x,y); assert(pp.pieces == 4 && pp.order == 4);
        assert(norm(ppval(pp, x) - y) < 1e-12);
        assert(norm(spline(x,[0 y 0],[1 8]) - [3 5]) < 1e-12);
        % clamped: end slope honoured
        pp = spline(x,[2 y -1]); d = ppval(pp, 1+1e-7) - ppval(pp, 1); assert(abs(d/1e-7 - 2) < 1e-4);
        % exact cubic reproduced
        t = 0:5; c = @(z) z.^3 - 2*z.^2 + z; assert(norm(spline(t, c(t), [0.5 2.5 4.2]) - c([0.5 2.5 4.2])) < 1e-10);
        assert(norm(spline([0 1],[0 2],[0.5 2]) - [1 4]) < 1e-12);
        assert(norm(spline([0 1 3],[0 2 1],[0.5 2 4]) - [1.20833333 2.33333333 -2]) < 1e-7);
        % pchip is shape preserving: monotone data stays monotone
        v = pchip(1:6,[0 0 1 1 2 5],1:0.25:6); assert(all(diff(v) >= -1e-14));
        assert(norm(pchip(x,y,x) - y) < 1e-14);
        assert(norm(makima(x,y,x) - y) < 1e-14);
        m = makima(0:3, reshape(1:24,[2 3 4]), [0 1 3]); assert(isequal(size(m),[2 3 3]));
        % mkpp/ppval/unmkpp round trip
        pp = mkpp([0 1 2],[1 0 0; 2 1 0]); assert(isequal(ppval(pp,[0.5 1.5 3]), [0.25 1 10]));
        [b,c,l,k,d] = unmkpp(pp); assert(isequal(b,[0 1 2]) && l == 2 && k == 3 && d == 1);
        pp = mkpp([0 1],[1 2; 3 4],2); assert(isequal(size(ppval(pp,[0.5 1])),[2 2]));
        failed = false; try, spline([1 2 3],[1 2 3 4]); catch, failed = true; end; assert(failed);
        failed = false; try, spline([1 1 2],[1 2 3]); catch, failed = true; end; assert(failed);"#);
}

#[test]
fn interp2_linear_nearest() {
    run(r#"[X,Y] = meshgrid(1:4,1:3); Z = X.^2 + 3*Y;
        assert(norm(interp2(X,Y,Z,[1.5 2.2 3.9],[1.2 2.5 1]) - [6.1 12.5 18.3]) < 1e-12);
        assert(isequal(size(interp2(1:4,1:3,Z,[1.5 2.2],[1.2;2.5])), [2 2]));
        assert(isequal(interp2(X,Y,Z,[1.4 2.6],[1.4 2.6],'nearest'), [4 18]));
        v = interp2(Z,[0.5 2],[1 2]); assert(isnan(v(1)) && v(2) == 10);
        assert(isequal(interp2(X,Y,Z,[0 2],[1 2],'linear',99), [99 10]));"#);
}

#[test]
fn polyfit_polyval() {
    run(r#"x = 0:6; y = [1 3 2 5 4 7 8];
        p = polyfit(x,y,1); assert(norm(p - [1.10714286 0.964285714]) < 1e-7);
        [p,S,mu] = polyfit(x,y,2);
        assert(norm(p - [0.388888889 2.39170192 3.95238095]) < 1e-7);
        assert(S.df == 4 && abs(S.normr - 2.1269249) < 1e-6 && isequal(size(S.R),[3 3]));
        assert(isequal(size(mu),[2 1]) && abs(mu(1) - 3) < 1e-14 && abs(mu(2) - 2.1602469) < 1e-7);
        % exact polynomial recovered
        t = linspace(-1,2,9); c = [2 -3 0.5 1]; assert(norm(polyfit(t, polyval(c,t), 3) - c) < 1e-9);
        % delta: sqrt(1 + sum((v/R).^2)) * normr / sqrt(df)
        [p,S] = polyfit(x,y,3); [yf,dl] = polyval(p,[1.5 4],S);
        V = vander(x(:),4); [~,R] = qr(V,0); nr = norm(y(:)-V*p(:)); r = zeros(1,2);
        pts = [1.5 4]; for k = 1:2, v = pts(k).^(3:-1:0); r(k) = sqrt(1+sum((v/R).^2))*nr/sqrt(3); end
        assert(norm(dl - r) < 1e-10);
        [p,S,mu] = polyfit(x,y,2); yf = polyval(p,[1.5 4],[],mu); assert(norm(yf - [2.47916667 5.14285714]) < 1e-7);
        failed = false; try, polyfit(1:3,1:4,1); catch, failed = true; end; assert(failed);"#);
}

#[test]
fn windows() {
    run(
        r#"assert(norm(hamming(7)' - [0.08 0.31 0.77 1 0.77 0.31 0.08]) < 1e-12);
        assert(norm(hamming(6,'periodic')' - [0.08 0.31 0.77 1 0.77 0.31]) < 1e-12);
        assert(isequal(size(hamming(0)),[0 1]) && hamming(1) == 1);
        assert(norm(hann(7)' - [0 0.25 0.75 1 0.75 0.25 0]) < 1e-12);
        assert(norm(hann(6,'periodic')' - [0 0.25 0.75 1 0.75 0.25]) < 1e-12);
        assert(norm(hanning(3)' - [0.5 1 0.5]) < 1e-12);
        assert(norm(blackman(5)' - [0 0.34 1 0.34 0]) < 1e-12);
        assert(norm(bartlett(5)' - [0 0.5 1 0.5 0]) < 1e-12);
        assert(isequal(rectwin(4), ones(4,1)));
        assert(norm(triang(5)' - [1 2 3 2 1]/3) < 1e-12);
        w = kaiser(7,4); assert(abs(w(4) - 1) < 1e-14 && abs(w(1) - 0.0884805261) < 1e-9 && abs(w(1)-w(7)) < 1e-15);
        assert(norm(kaiser(5) - kaiser(5,0.5)) == 0);
        failed = false; try, hamming(5,'bogus'); catch, failed = true; end; assert(failed);
        failed = false; try, hamming(-1); catch, failed = true; end; assert(failed);"#,
    );
}

#[test]
fn quantiles_and_percentiles() {
    run(r#"x = [3 1 4 1 5 9 2 6];
        assert(norm(quantile(x,[0 0.25 0.5 0.75 1]) - [1 1.5 3.5 5.5 9]) < 1e-12);
        assert(abs(quantile([1 2 3 4],0.5) - 2.5) < 1e-14);
        assert(norm(prctile(x,[10 50 90]) - [1 3.5 8.1]) < 1e-12);
        assert(norm(quantile(x,[0.1 0.9],2,7) - [1 6.9]) < 1e-12);
        A = [1 5 3; 4 2 8; 7 7 1; 2 9 4];
        assert(isequal(size(quantile(A,[0.2 0.6])),[2 3]));
        assert(isequal(size(prctile(A,50,2)),[4 1]));
        assert(isequal(size(prctile(A)),[5 3]));
        assert(abs(quantile([1 NaN 3 4 NaN],0.5) - 3) < 1e-14);
        assert(abs(iqr([1 2 3 4]) - 2) < 1e-14 && isequal(size(iqr(A)),[1 3]));
        failed = false; try, quantile(x,1.5); catch, failed = true; end; assert(failed);
        assert(quantile(x, 0.5, 2, 1) == 3 && quantile(x, 0.5, 2, 2) == 3.5);"#);
}

#[test]
fn covariance_correlation() {
    run(r#"A = [1 5 3; 4 2 8; 7 7 1; 2 9 4];
        C = cov(A); assert(isequal(size(C),[3 3]) && abs(C(1,1) - var(A(:,1))) < 1e-12);
        assert(abs(cov([1 2 3 4]) - 5/3) < 1e-14);
        C = cov([1 2 3 4],[2 4 6 8]); assert(isequal(size(C),[2 2]) && abs(C(1,2) - 10/3) < 1e-12);
        assert(abs(cov([1 2 3 4],1) - 1.25) < 1e-14);
        R = corrcoef(A); assert(all(diag(R) == 1) && abs(R(1,2) - R(2,1)) < 1e-15);
        R = corrcoef([1 2 3 4],[2 4 6 8]); assert(abs(R(1,2) - 1) < 1e-12);
        [R,P,L,U] = corrcoef([1 2 3 4 5 6]', [2 1 4 3 7 5]');
        assert(abs(R(1,2) - 0.7918) < 1e-4 && abs(P(1,2) - 0.060511) < 1e-5 && L(1,2) < R(1,2) && U(1,2) > R(1,2));
        B = [1 2 NaN; 3 5 6; 7 1 2; 4 4 4; 2 2 9];
        Rc = corrcoef(B,'Rows','complete'); assert(isequal(size(Rc),[3 3]) && ~any(isnan(Rc(:))));
        Rp = corrcoef(B,'Rows','pairwise'); assert(~any(isnan(Rp(:))));
        assert(any(isnan(corrcoef(B))(:)));"#);
}

#[test]
fn var_std_options() {
    run(r#"A = [1 5 3; 4 2 8; 7 7 1; 2 9 4];
        assert(norm(var(A) - [7 8.9166666667 8.6666666667], Inf) < 1e-9);
        assert(norm(var(A,1,2) - var(A,1,2)) == 0 && isequal(size(var(A,0,2)),[4 1]));
        w = [1 2 3 4]'; m = sum(w.*A)/sum(w); v = sum(w.*(A-m).^2)/sum(w); assert(norm(var(A,w) - v) < 1e-12);
        assert(abs(var([1 NaN 3 5],'omitnan') - 4) < 1e-14 && isnan(var([1 NaN 3 5])));
        assert(abs(var(A,0,'all') - var(A(:))) < 1e-14);
        [s,m2] = std([1 2 3 4]); assert(abs(s - sqrt(5/3)) < 1e-14 && m2 == 2.5);
        assert(std(5) == 0);
        failed = false; try, var(A,[1 2]'); catch, failed = true; end; assert(failed);"#);
}

#[test]
fn normalize_and_cum() {
    run(r#"assert(norm(normalize([1 2 3]) - [-1 0 1]) < 1e-14);
        assert(norm(normalize([1 2 3 5],'range') - [0 0.25 0.5 1]) < 1e-14);
        assert(norm(normalize([1 2 3 5],'range',[-1 1]) - [-1 -0.5 0 1]) < 1e-14);
        assert(norm(normalize([3 4],'norm') - [0.6 0.8]) < 1e-14);
        assert(norm(normalize([1 2 3],'center') - [-1 0 1]) < 1e-14);
        assert(norm(normalize([1 2 3],'scale','first') - [1 2 3]) < 1e-14);
        [n,c,s] = normalize([1 2 3]); assert(c == 2 && s == 1);
        assert(norm(normalize([1 2; 3 6],2) - [-1 1; -1 1]/sqrt(2)) < 1e-14);
        [m,i] = cummax([1 3 2 5 4]); assert(isequal(m,[1 3 3 5 5]) && isequal(i,[1 2 2 4 4]));
        assert(isequal(cummin([5 3 4 1],'reverse'), [1 1 1 1]));
        m = cummax([NaN 2 1]); assert(isnan(m(1)) && m(2) == 2 && m(3) == 2);"#);
}

#[test]
fn complex_linear_algebra() {
    run(r#"B = [1 2i; 3 4]; C = [1 2+1i 3; 4i 5 6; 7 8 9-2i];
        assert(abs(det(B) - (4 - 6i)) < 1e-14);
        assert(norm(inv(C)*C - eye(3)) < 1e-12);
        assert(norm(C\[1;2;3] - inv(C)*[1;2;3]) < 1e-12);
        assert(norm([1 2 3]/C - [1 2 3]*inv(C)) < 1e-12);
        assert(rank([1 2i; 2 4i]) == 1 && rank(C) == 3);
        [Q,R] = qr(C); assert(norm(Q*R - C) < 1e-12 && norm(Q'*Q - eye(3)) < 1e-12 && isreal(diag(R)));
        [Q,R,P] = qr(C); assert(norm(Q*R - C*P) < 1e-12);
        [Q,R] = qr([1 2i;3 4;5 6],0); assert(isequal(size(Q),[3 2]));
        [L,U,P] = lu(C); assert(norm(P*C - L*U) < 1e-12);
        [L,U] = lu(C); assert(norm(L*U - C) < 1e-12);
        [U,S,V] = svd(C); assert(norm(U*S*V' - C) < 1e-12);
        s = svd(C); assert(all(diff(s) <= 0) && abs(norm(C) - s(1)) < 1e-12);
        [U,S,V] = svd([1 2i 3; 4 5 6i]); assert(norm(U*S*V' - [1 2i 3; 4 5 6i]) < 1e-12 && isequal(size(S),[2 3]));
        N = null([1 1i 2]); assert(isequal(size(N),[3 2]) && norm([1 1i 2]*N) < 1e-12);
        O = orth([1 1i; 1i -1]); assert(size(O,2) == 1);
        P = pinv([1 1i; 1i 2; 3 4]); A = [1 1i; 1i 2; 3 4]; assert(norm(A*P*A - A) < 1e-12);
        assert(abs(cond([1 2;3 4]) - 14.933034373659268) < 1e-9 && abs(cond([1 2;3 4],1) - 21) < 1e-12);
        assert(abs(cond([1 2;3 4],inf) - 21) < 1e-12 && abs(cond([1 2;3 4],'fro') - 15) < 1e-12);
        assert(abs(norm([1 2;3 4]) - 5.464985704219043) < 1e-13);
        assert(abs(trace([1+2i 0; 0 3-1i]) - (4+1i)) < 1e-15);
        K = kron([1 2i],[1;3]); assert(isequal(K, [1 2i; 3 6i]));
        assert(isequal(kron([1 2],[1 2],[1 2]), [1 2 2 4 2 4 4 8]));"#);
}

#[test]
fn chol_sqrtm_logm() {
    run(
        r#"R = chol([4 2;2 3]); assert(norm(R'*R - [4 2;2 3]) < 1e-14 && R(2,1) == 0);
        L = chol([4 2;2 3],'lower'); assert(norm(L*L' - [4 2;2 3]) < 1e-14 && L(1,2) == 0);
        A = [5 1+2i 0; 1-2i 6 1; 0 1 3]; R = chol(A); assert(norm(R'*R - A) < 1e-12);
        [R,p] = chol([1 2;2 1]); assert(p == 2 && isequal(size(R),[1 1]));
        failed = false; try, chol([1 2;2 1]); catch, failed = true; end; assert(failed);
        S = sqrtm([4 1;0 9]); assert(norm(S*S - [4 1;0 9]) < 1e-12);
        S = sqrtm([5 2;2 5]); assert(norm(S*S - [5 2;2 5]) < 1e-12);
        assert(abs(sqrtm(-4) - 2i) < 1e-14);
        D = sqrtm(diag([4 -9])); assert(abs(D(2,2) - 3i) < 1e-14);
        M = [2 1;0 3]; L = logm(M); assert(norm(expm(L) - M) < 1e-12);
        M = [5 2;2 5]; assert(norm(expm(logm(M)) - M) < 1e-12);
        Cc = [1 2+1i 3; 4i 5 6; 7 8 9-2i]; assert(norm(sqrtm(Cc)^2 - Cc) < 1e-9);"#,
    );
}

#[test]
fn bit_functions() {
    run(r#"assert(isequal(bitcmp(uint8(5)), uint8(250)));
        assert(isequal(bitcmp(uint16([1 2 65535])), uint16([65534 65533 0])));
        assert(isequal(bitcmp(int8([0 5 -3])), int8([-1 -6 2])));
        assert(bitcmp(5) == 2^53 - 1 - 5);
        assert(isequal(bitcmp(5,'uint8'), 250));
        failed = false; try, bitcmp(uint16(1),8); catch, failed = true; end; assert(failed);
        assert(isequal(bitshift(uint8(5),2), uint8(20)) && isequal(bitshift(uint8(255),-3), uint8(31)));
        assert(isequal(bitshift(uint8(200),2), uint8(32)));
        assert(isequal(bitshift(5,[1 2 -1]), [10 20 2]));
        assert(isequal(idivide(int32([7 -7]),int32(2),'floor'), int32([3 -4])));
        assert(isequal(idivide(int32([7 -7]),int32(2)), int32([3 -3])));
        assert(isequal(idivide(int32([7 -7]),int32(2),'ceil'), int32([4 -3])));
        assert(isequal(idivide(int32([7 -7 5]),int32(2),'round'), int32([4 -4 3])));"#);
}

#[test]
fn periodogram_basic() {
    run(r#"fs = 1000; t = (0:999)/fs; x = sin(2*pi*100*t);
        [P,f] = periodogram(x,[],[],fs); [~,i] = max(P); assert(abs(f(i) - 100) < 2);
        assert(f(1) == 0 && abs(f(end) - fs/2) < 1e-9);
        % Parseval: one-sided PSD integrates to the signal power
        df = f(2) - f(1); assert(abs(sum(P)*df - mean(x.^2)) < 1e-3);
        [P,f] = periodogram([1 2 3 4 5 6 7 8]); assert(isequal(size(P),[129 1]) && abs(f(2) - pi/128) < 1e-14);
        [P,f] = periodogram([1 2 3 4 5 6 7 8],[],16,100);
        assert(norm(P' - [1.62 1.7451637 0.27313708 0.16855175 0.08 0.073630794 0.046862915 0.052653743 0.02]) < 1e-6);
        [P,f] = periodogram([1 2 3 4 5 6 7 8],hamming(8),32); assert(abs(P(1) - 17.222486) < 1e-5);
        [P,f] = periodogram([1 2 3 4 5 6 7 8],[],16,'twosided'); assert(numel(P) == 16);
        [P,f] = periodogram([1 2 3 4 5 6 7 8],[],16,'power'); assert(abs(P(1) - 20.25) < 1e-12);"#);
}

#[test]
fn poly_helpers_and_moving_windows() {
    run(
        r#"assert(isequal(polyder([1 2 3 4]), [3 4 3]) && isequal(polyder(5), 0));
        assert(isequal(polyder([1 2],[1 3]), [2 5]));
        [q,d] = polyder([1 2],[1 3 1]); assert(isequal(q,[-1 -4 -5]) && isequal(d,[1 6 11 6 1]));
        assert(isequal(polyint([3 2 1]), [1 1 1 0]) && isequal(polyint([3 2 1],5), [1 1 1 5]));
        assert(isequal(fftshift(1:6), [4 5 6 1 2 3]) && isequal(fftshift(1:5), [4 5 1 2 3]));
        assert(isequal(ifftshift(1:5), [3 4 5 1 2]));
        assert(isequal(movmedian([1 9 2 8 3],3), [5 2 8 3 5.5]));
        assert(isequal(movmax([3 1 4 1 5],[1 0]), [3 3 4 4 5]));
        assert(isequal(movmedian(1:5,3,'Endpoints','discard'), [2 3 4]));
        assert(isequal(movmedian(1:3,3,'Endpoints',0), [1 2 2]));
        assert(abs(movstd([1 2 4],3)(2) - std([1 2 4])) < 1e-14);"#,
    );
}

#[test]
fn del2_laplacian() {
    run(
        r#"assert(norm(del2([1 4 9 16 25 36]) - 0.5*ones(1,6)) < 1e-14);
        [X,Y] = meshgrid(1:5,1:5); L = del2(X.^2 + Y.^2); assert(norm(L(:) - 1) < 1e-12);"#,
    );
}
