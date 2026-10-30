function grafik_oracle(cases_file, out_dir, name)
  % Runs ONE oracle case from cases.txt in this (fresh) Octave process:
  % fltk toolkit, invisible figure, eval body, write jsonencode(__mf_figure_data__(gcf)).
  % On error writes <name>.error.txt instead. Called by grafik_oracle.sh.
  here = fileparts(mfilename('fullpath'));
  root = fileparts(fileparts(here));
  ws = warning();
  warning('off', 'all');
  addpath(fullfile(root, 'octave'));
  if strcmp(getenv('GRAFIK_COMPAT'), '1'), addpath(fullfile(root, 'octave', 'compat')); end
  tk = getenv('GRAFIK_TOOLKIT'); if isempty(tk), tk = 'fltk'; end
  graphics_toolkit(tk);
  warning(ws);
  % Shadow warnings for IndyMAT's input hooks are noise here.
  warning('off', 'Octave:shadowed-function');
  cases = parse_cases(cases_file);
  idx = find(strcmp({cases.name}, name));
  if isempty(idx), error('grafik_oracle: unknown case "%s"', name); end
  body = cases(idx).body;
  json_path = fullfile(out_dir, [name '.json']);
  err_path = fullfile(out_dir, [name '.error.txt']);
  if exist(json_path, 'file'), delete(json_path); end
  if exist(err_path, 'file'), delete(err_path); end
  close all;
  try
    figure('visible', 'off');
    if ~isempty(strtrim(body))
      evalc(body);
    end
    bytes = jsonencode(__mf_figure_data__(gcf));
    fid = fopen(json_path, 'w'); fputs(fid, bytes); fputs(fid, "\n"); fclose(fid);
    printf('ok    %s\n', name);
  catch err
    fid = fopen(err_path, 'w'); fputs(fid, [err.message "\n"]); fclose(fid);
    printf('FAIL  %s: %s\n', name, err.message);
  end
  close all;
end

function cases = parse_cases(path)
  text = fileread(path);
  lines = strsplit(strrep(text, "\r", ''), "\n");
  cases = struct('name', {}, 'body', {});
  cur = 0;
  for i = 1:numel(lines)
    l = lines{i};
    t = strtrim(l);
    if strncmp(t, '%%', 2)
      cur = numel(cases) + 1;
      cases(cur).name = strtrim(t(3:end));
      cases(cur).body = '';
    elseif strncmp(t, '%', 1)
      continue;   % comment line
    elseif cur > 0
      cases(cur).body = [cases(cur).body l "\n"];
    end
  end
end
