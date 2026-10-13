function [payload, supported] = uy_ozet_nesne(x, budget)
% Public semantic payloads for exact categorical, duration, calendarDuration,
% unzoned datetime and table classes.
% Caller serializes the built-in payload with its remaining shared budget.
% No display rendering, private storage, or inferred object completeness.
% Public contracts: MathWorks categorical/double, categories, isordinal,
% isprotected; duration/seconds and duration Format property.
% https://www.mathworks.com/help/matlab/ref/categorical.html
% https://www.mathworks.com/help/matlab/ref/duration.html
% https://www.mathworks.com/help/matlab/ref/duration.seconds.html
% https://www.mathworks.com/help/matlab/ref/table.html
  payload = []; supported = false;
  if nargin ~= 2 || ~isnumeric(budget) || ~isreal(budget) || ...
      ~isscalar(budget) || ~isfinite(budget) || budget < 1 || ...
      budget ~= fix(budget)
    return;
  end
  kind = class(x);
  if ~strcmp(kind, 'categorical') && ~strcmp(kind, 'duration') && ...
      ~strcmp(kind, 'table') && ~strcmp(kind, 'datetime') && ...
      ~strcmp(kind, 'calendarDuration'), return; end
  try
    % Check before converting object values or constructing payload arrays.
    if numel(x) > budget, return; end
    if strcmp(kind,'table')
      if size(x,2) > budget, return; end
      public = {};
    else
      public = properties(x);
    end
    if strcmp(kind, 'categorical')
      if ~isempty(public), return; end
      names = categories(x);
      if ~iscell(names) || numel(names) > budget, return; end
      remaining = budget - numel(names);
      for k = 1:numel(names)
        if ~ischar(names{k}) || ~isrow(names{k}) || ...
            numel(names{k}) > remaining
          return;
        end
        remaining = remaining - numel(names{k});
      end
      codes = double(x);
      ordinal = isordinal(x); protected = isprotected(x);
      if ~isa(codes, 'double') || ~isequal(size(codes), size(x)) || ...
          ~islogical(ordinal) || ~isscalar(ordinal) || ...
          ~islogical(protected) || ~isscalar(protected)
        return;
      end
      payload = struct('Codes', codes, 'Categories', {names(:)}, ...
                       'Ordinal', ordinal, 'Protected', protected);
    elseif strcmp(kind, 'duration')
      if numel(public) ~= 1 || ~strcmp(public{1}, 'Format'), return; end
      format = x.Format;
      if ~ischar(format) || ~isrow(format) || numel(format) > budget, return; end
      values = seconds(x);
      if ~isa(values, 'double') || ~isequal(size(values), size(x)), return; end
      payload = struct('Seconds', values, 'Format', format);
    elseif strcmp(kind, 'calendarDuration')
      % R2025b olc26-object-next: split months/days/time preserves the
      % independent calendar components, including signed months and days.
      % Never replace calendar months by an assumed number of seconds.
      if numel(public) ~= 1 || ~strcmp(public{1}, 'Format'), return; end
      format = x.Format;
      if ~ischar(format) || ~isrow(format) || numel(format) > budget, return; end
      [months,days,time] = split(x,{'months','days','time'});
      secs = seconds(time);
      for value = {months,days,secs}
        if ~isa(value{1},'double') || ~isequal(size(value{1}),size(x)) || ...
            any(~isfinite(value{1}(:))), return; end
      end
      z = zeros(size(months));
      reconstructed = calendarDuration(z,months,days,z,z,secs);
      reconstructed.Format = format;
      if ~isequal(reconstructed,x), return; end
      payload = struct('Months',months,'Days',days,'Seconds',secs,'Format',format);
    elseif strcmp(kind, 'datetime')
      allowed = {'Year','Month','Day','Hour','Minute','Second','Format','TimeZone'};
      if ~all(ismember(public,allowed)) || ...
          ~all(ismember({'Format','TimeZone'},public)), return; end
      zone = x.TimeZone; format = x.Format;
      if ~ischar(zone) || ~isempty(zone) || ~ischar(format) || ...
          ~isrow(format) || numel(format) > budget, return; end
      % Six bounded public component arrays, no datenum or display parsing.
      [Y,M,D] = ymd(x); [h,m,s] = hms(x);
      components = {Y,M,D,h,m,s};
      for k = 1:6
        value = components{k};
        if ~isa(value,'double') || ~isequal(size(value),size(x)) || ...
            any(~isfinite(value(:))), return; end
      end
      reconstructed = datetime(Y,M,D,h,m,s);
      reconstructed.Format = format;
      % Exact native equality proves that extraction did not discard precision.
      if ~isequal(reconstructed,x), return; end
      payload = struct('Year',Y,'Month',M,'Day',D,'Hour',h, ...
                       'Minute',m,'Second',s,'Format',format,'TimeZone',zone);
    end
    if strcmp(kind, 'table')
      P = x.Properties;
      if isstruct(P), fields = fieldnames(P); else, fields = properties(P); end
      required = {'VariableNames','RowNames','DimensionNames','Description', ...
                  'VariableUnits','VariableDescriptions','UserData', ...
                  'VariableContinuity','CustomProperties'};
      allowed = [required {'VariableTypes'}];
      if ~all(ismember(required, fields)) || ~all(ismember(fields, allowed))
        return;
      end
      custom = P.CustomProperties;
      if isstruct(custom), customnames = fieldnames(custom);
      elseif isobject(custom), customnames = properties(custom);
      elseif isempty(custom), customnames = {};
      else, return;
      end
      if ~isempty(customnames) || ~isempty(P.VariableContinuity), return; end
      metadata = struct();
      textfields = {'VariableNames','RowNames','DimensionNames', ...
                    'VariableUnits','VariableDescriptions'};
      remaining = budget;
      for k = 1:numel(textfields)
        value = P.(textfields{k});
        if isa(value, 'string')
          if numel(value) > remaining || any(ismissing(value(:))), return; end
          value = cellstr(value);
        end
        if ~iscell(value) || numel(value) > remaining || ...
            ~all(cellfun(@ischar, value(:))), return; end
        [ok,remaining] = uy_plain(value, remaining, 0);
        if ~ok, return; end
        metadata.(textfields{k}) = value;
      end
      metadata.Description = P.Description;
      metadata.UserData = P.UserData;
      metadata.VariableContinuity = P.VariableContinuity;
      for name = {'Description','UserData','VariableContinuity'}
        [ok,remaining] = uy_plain(metadata.(name{1}),remaining,0);
        if ~ok, return; end
      end
      names = metadata.VariableNames;
      if numel(names) > remaining, return; end
      values = cell(1,numel(names)); types = cell(1,numel(names));
      for k = 1:numel(names)
        values{k} = x.(names{k}); types{k} = class(values{k});
        [ok,remaining] = uy_plain(values{k},remaining,0);
        if ~ok, return; end
      end
      % VariableTypes is derived from the preserved values, also in releases
      % where the public metadata field is absent. Refuse inconsistent types.
      if any(strcmp(fields,'VariableTypes'))
        declared = P.VariableTypes;
        if isa(declared,'string'), declared = cellstr(declared); end
        if ~iscell(declared) || ~isequal(declared(:),types(:)), return; end
      end
      metadata.VariableTypes = types;
      metadata.CustomProperties = struct();
      payload = struct('Values',{values},'Properties',metadata);
    end
    supported = true;
  catch
    % Missing public API or unexpected dispatch remains explicit partial.
    payload = []; supported = false;
  end
end

function [ok, budget] = uy_plain(value, budget, depth)
% Only finite-depth value containers; no handles, objects or dispatch cycles.
  ok = false;
  if depth > 8 || budget < 1 || numel(value) > budget, return; end
  budget = budget - 1;
  if isnumeric(value) || islogical(value) || ischar(value)
    if numel(value) > budget, return; end
    budget = budget - numel(value); ok = true;
  elseif iscell(value)
    for k = 1:numel(value)
      [valid,budget] = uy_plain(value{k},budget,depth+1);
      if ~valid, return; end
    end
    ok = true;
  elseif isstruct(value) && isscalar(value)
    fields = fieldnames(value);
    if numel(fields) > budget, return; end
    for k = 1:numel(fields)
      [valid,budget] = uy_plain(value.(fields{k}),budget,depth+1);
      if ~valid, return; end
    end
    ok = true;
  end
end
