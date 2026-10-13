% Matematik: dalga 26 gerçek MATLAB tablosuyla norm alt kümesi
%% wave26-norm-measured-vectors | norm | R2025b round2: skaler, çok eleman, sıfır, NaN ve single
xs={0,1,[0 0],[3 0 -2],single([3 0 -2]),[NaN 1]};
zero={1,1,Inf,Inf,single(Inf),NaN}; negative={0,1,0,0,single(0),NaN};
for k=1:numel(xs)
  actual=norm(xs{k},0); assert(isequaln(actual,zero{k}));
  assert(strcmp(class(actual),class(xs{k})));
  for p=[-1 -2]
    actual=norm(xs{k},p); assert(isequaln(actual,negative{k}));
    assert(strcmp(class(actual),class(xs{k})));
  end
end
%% wave26-norm-measured-nonvectors | norm | R2025b round2: boş matris ve 2x2 matris reddi
for x={[],[3 0;0 -2]}
  for p=[0 -1 -2 -Inf]
    failed=false; try, norm(x{1},p); catch e, failed=strcmp(e.identifier,'MATLAB:norm:unknownNorm'); end
    assert(failed);
  end
end
