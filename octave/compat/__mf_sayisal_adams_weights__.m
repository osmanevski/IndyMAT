function [w, remainder] = __mf_sayisal_adams_weights__ (nodes)
% Integrals over [0,1] of Lagrange basis on dimensionless time nodes.
% Remainder multiplier is integral(prod(s-nodes))/factorial(order).
  n=numel(nodes); w=zeros(1,n);
  for j=1:n
    others=nodes([1:j-1 j+1:n]); p=1;
    for z=others, p=conv(p,[1 -z]); endfor
    w(j)=sum(p./(numel(p):-1:1))/prod(nodes(j)-others);
  endfor
  p=1;
  for z=nodes, p=conv(p,[1 -z]); endfor
  remainder=sum(p./(numel(p):-1:1))/factorial(n);
endfunction
