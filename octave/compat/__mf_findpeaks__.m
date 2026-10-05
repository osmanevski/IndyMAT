function [peaks, locations, widths, prominences] = __mf_findpeaks__ (data, varargin)
% __MF_FINDPEAKS__ MATLAB peak prominence and half-prominence width subset.
% Finite real double vectors; height, prominence, distance, width and threshold
% filters, NPeaks and SortStr. Flat peaks use their first sample. Endpoints
% are excluded. Explicit locations, sampling rates, Inf peaks, DoubleSided
% and alternative width references are unsupported and rejected.
  if (! isa (data, 'double') || ! isreal (data) || ! isvector (data) || numel (data) < 3 || any (! isfinite (data(:))))
    error ('findpeaks: data must be a finite real double vector with at least three samples');
  endif
  row = isrow (data); y = double (data(:)); n = numel (y);
  opt = struct ('MinPeakHeight', -Inf, 'MinPeakProminence', 0, ...
                'MinPeakDistance', 0, 'MinPeakWidth', 0, ...
                'MaxPeakWidth', Inf, 'Threshold', 0, 'NPeaks', Inf);
  names = fieldnames (opt); order = 'none';
  if (rem (numel (varargin), 2)), error ('findpeaks: options must be name/value pairs'); endif
  for j = 1:2:numel (varargin)
    key = varargin{j}; value = varargin{j+1};
    if (! ischar (key)), error ('findpeaks: option names must be character vectors'); endif
    if (strcmpi (key, 'SortStr'))
      if (! ischar (value) || ! any (strcmpi (value, {'none','ascend','descend'})))
        error ('findpeaks: SortStr must be none, ascend or descend');
      endif
      order = lower (value); continue;
    endif
    match = find (strcmpi (key, names), 1);
    if (isempty (match)), error ('findpeaks: unsupported option %s', key); endif
    name = names{match};
    if (! isnumeric (value) || ! isreal (value) || ! isscalar (value) || isnan (value))
      error ('findpeaks: %s must be a real scalar', name);
    endif
    if (! strcmp (name, 'MinPeakHeight') && value < 0)
      error ('findpeaks: %s must be nonnegative', name);
    endif
    if (strcmp (name, 'NPeaks') && (value < 1 || fix (value) != value))
      error ('findpeaks: NPeaks must be a positive integer');
    endif
    opt.(name) = value;
  endfor
  locations = []; ends = []; j = 2;
  while (j < n)
    last = j;
    while (last < n && y(last+1) == y(j)), last++; endwhile
    if (last < n && y(j) > y(j-1) && y(j) > y(last+1))
      locations(end+1,1) = j; ends(end+1,1) = last;
    endif
    j = last+1;
  endwhile
  peaks = y(locations); prominences = zeros (size (peaks)); widths = prominences;
  for j = 1:numel (locations)
    first = locations(j); last = ends(j); height = peaks(j);
    left = first-1;
    while (left > 1 && y(left-1) <= height), left--; endwhile
    right = last+1;
    while (right < n && y(right+1) <= height), right++; endwhile
    base = max (min(y(left:first)), min(y(last:right)));
    prominences(j) = height-base;
    level = height-prominences(j)/2;
    a = first;
    while (a > left && y(a-1) > level), a--; endwhile
    b = last;
    while (b < right && y(b+1) > level), b++; endwhile
    crossleft = a;
    if (a > left), crossleft -= (y(a)-level)/(y(a)-y(a-1)); endif
    crossright = b;
    if (b < right), crossright += (y(b)-level)/(y(b)-y(b+1)); endif
    widths(j) = crossright-crossleft;
  endfor
  % Distance suppression visits the tallest candidates first, including ties
  % at the distance boundary (the project's established MATLAB convention).
  keep = true (size (peaks)); [~, ranked] = sort (peaks, 'descend');
  for j = ranked(:)'
    if (! keep(j)), continue; endif
    conflict = abs (locations-locations(j)) <= opt.MinPeakDistance;
    conflict(j) = false; keep(conflict) = false;
  endfor
  neighbor_drop = min (peaks-y(locations-1), peaks-y(ends+1));
  keep &= peaks >= opt.MinPeakHeight & prominences >= opt.MinPeakProminence ...
          & widths >= opt.MinPeakWidth & widths <= opt.MaxPeakWidth ...
          & neighbor_drop >= opt.Threshold;
  peaks = peaks(keep); locations = locations(keep);
  widths = widths(keep); prominences = prominences(keep);
  if (! strcmp (order, 'none'))
    [peaks, permutation] = sort (peaks, order);
    locations = locations(permutation); widths = widths(permutation);
    prominences = prominences(permutation);
  endif
  count = min (numel (peaks), opt.NPeaks);
  peaks = peaks(1:count); locations = locations(1:count);
  widths = widths(1:count); prominences = prominences(1:count);
  if (row)
    peaks = peaks.'; locations = locations.'; widths = widths.'; prominences = prominences.';
  endif
endfunction
