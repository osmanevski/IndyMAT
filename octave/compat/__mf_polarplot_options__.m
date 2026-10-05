function [spec,props]=__mf_polarplot_options__(args)
% Parse a single MATLAB-style line specification and whitelisted properties.
  keys={'color','linewidth','linestyle','marker','markersize','markeredgecolor','markerfacecolor'};
  spec=''; props={};
  if ~isempty(args) && ischar(args{1}) && ~any(strcmpi(args{1},keys))
    candidate=args{1}; rest=candidate;
    % Each category occurs at most once, in arbitrary order.
    rest=regexprep(rest,'--|-\.|-|:', '', 'once');
    rest=regexprep(rest,'[rgbcmykw]', '', 'once');
    rest=regexprep(rest,'[+o*.xsd^v><ph]', '', 'once');
    if isempty(candidate) || ~isempty(rest), error('polarplot: invalid LineSpec or unsupported option %s',candidate); endif
    spec=candidate; args(1)=[];
  endif
  if mod(numel(args),2), error('polarplot: line options must be name/value pairs'); endif
  for k=1:2:numel(args)
    if ~ischar(args{k}) || ~any(strcmpi(args{k},keys)), error('polarplot: unsupported line property'); endif
    props=[props args(k:k+1)];
  endfor
endfunction
