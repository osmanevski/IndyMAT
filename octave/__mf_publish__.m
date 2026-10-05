function output_file = __mf_publish__(source_file)
% __MF_PUBLISH__ Stage a report privately; Python installs only validated HTML.
  source_dir=fileparts(source_file);
  [~,source_name,source_ext]=fileparts(source_file);
  if ~isappdata(0,'__mf_folder__'),error(__mf_text__('Publishing requires a server job.', 'Yayın için sunucu işi gerekli.'));endif
  output_dir=fullfile(getappdata(0,'__mf_folder__'),'publish');
  previous_dir=pwd();
  had_publish_flag=isappdata(0,'__mf_publishing__');
  if had_publish_flag,previous_publish_flag=getappdata(0,'__mf_publishing__');endif
  setappdata(0,'__mf_publishing__',true);
  overlay_dir='';
  unwind_protect
    cd(source_dir);
    overlay_dir=__mf_publish_overlay__();
    addpath(overlay_dir,'-begin');
    clear publish;
    options=struct('format','html','outputDir',output_dir,'evalCode',true,'catchError',false,'showCode',true,'imageFormat','png','useNewFigure',false);
    staged_file=publish([source_name source_ext],options);
    __mf_sanitize_published_html__(staged_file);
    % This path is not written by Octave. The completion hook sanitises the
    % private staged file, then installs it under the server's file guards.
    output_file=fullfile(source_dir,'html',[source_name '.html']);
  unwind_protect_cleanup
    unwind_protect
      if ~isempty(overlay_dir)
        rmpath(overlay_dir);
        clear publish;
        rmdir(overlay_dir,'s');
      endif
    unwind_protect_cleanup
      unwind_protect
        if had_publish_flag
          setappdata(0,'__mf_publishing__',previous_publish_flag);
        elseif isappdata(0,'__mf_publishing__')
          rmappdata(0,'__mf_publishing__');
        endif
      unwind_protect_cleanup
        cd(previous_dir);
      end_unwind_protect
    end_unwind_protect
  end_unwind_protect
endfunction

function overlay_dir=__mf_publish_overlay__()
  publish_file=which('publish');
  if isempty(publish_file),error(__mf_text__('The HTML publisher was not found in this Octave installation.', 'Bu Octave kurulumunda HTML yayınlayıcısı bulunamadı.'));endif
  formatter_file=fullfile(fileparts(publish_file),'private','__publish_html_output__.m');
  if exist(formatter_file,'file')~=2,error(__mf_text__('The HTML formatter was not found in this Octave installation.', 'Bu Octave kurulumunda HTML biçimlendiricisi bulunamadı.'));endif
  source=fileread(publish_file);
  % Private function lookup outranks the published file's current directory.
  % The tracker handle/state stays in publish's caller context, not eval_context.
  old='    doc = eval_code (doc, options);';
  new=sprintf(['    mf_tracker = __mf_publish_figures__ ();\n' ...
               '    unwind_protect\n' ...
               '      doc = eval_code (doc, options, mf_tracker);\n' ...
               '    unwind_protect_cleanup\n' ...
               '      mf_tracker ("cleanup");\n' ...
               '    end_unwind_protect']);
  if numel(strfind(source,old))~=1,error(__mf_text__('Private publishing state is not supported in this Octave version.', 'Bu Octave sürümünde özel yayın durumu desteklenmiyor.'));endif
  source=strrep(source,old,new);
  old='function doc = eval_code (doc, options)';
  new='function doc = eval_code (doc, options, mf_tracker)';
  if numel(strfind(source,old))~=1,error(__mf_text__('A private publishing context is not supported in this Octave version.', 'Bu Octave sürümünde özel yayın bağlamı desteklenmiyor.'));endif
  source=strrep(source,old,new);
  old='      ## Check for newly created figures ...';
  new=sprintf('      fig_ids_changed = mf_tracker("end");\n\n      ## Check for newly created figures ...');
  if numel(strfind(source,old))~=1,error(__mf_text__('The publisher in this Octave version does not support figure capture.', 'Bu Octave sürümünün yayınlayıcısı grafik yakalama için desteklenmiyor.'));endif
  source=strrep(source,old,new);
  old='      fig_ids_new = setdiff (findall (0, "type", "figure"), fig_ids);';
  new='      fig_ids_new = fig_ids_changed(:);';
  if numel(strfind(source,old))~=1,error(__mf_text__('The publisher in this Octave version does not support figure listing.', 'Bu Octave sürümünün yayınlayıcısı grafik listesi için desteklenmiyor.'));endif
  source=strrep(source,old,new);
  old='        delete (fig_ids_new(j));';
  new='        ## Keep figures exactly as the evaluated code left them.';
  if numel(strfind(source,old))~=1,error(__mf_text__('The publisher in this Octave version does not support figure cleanup.', 'Bu Octave sürümünün yayınlayıcısı grafik temizliği için desteklenmiyor.'));endif
  source=strrep(source,old,new);
  old='  delete (setdiff (findall (0, "type", "figure"), fig_ids));';
  new='  ## useNewFigure is false: all remaining figures belong to user code.';
  if numel(strfind(source,old))~=1,error(__mf_text__('The publisher in this Octave version does not support final figure cleanup.', 'Bu Octave sürümünün yayınlayıcısı son grafik temizliği için desteklenmiyor.'));endif
  source=strrep(source,old,new);
  old='      r = doc.body{i}.lines;';
  new=sprintf('      mf_tracker("begin");\n      r = doc.body{i}.lines;');
  if numel(strfind(source,old))~=1,error(__mf_text__('The publisher in this Octave version does not support section evaluation.', 'Bu Octave sürümünün yayınlayıcısı bölüm değerlendirmesi için desteklenmiyor.'));endif
  source=strrep(source,old,new);
  old='        print (print_opts{:});';
  new='        mf_tracker("print", print_opts{:});';
  if numel(strfind(source,old))~=1,error(__mf_text__('The publisher in this Octave version does not support figure output.', 'Bu Octave sürümünün yayınlayıcısı grafik çıktısı için desteklenmiyor.'));endif
  source=strrep(source,old,new);
  overlay_dir=tempname();
  if ~mkdir(overlay_dir),error(__mf_text__('Could not create a temporary Octave directory for publishing.', 'Yayın için geçici Octave dizini oluşturulamadı.'));endif
  private_dir=fullfile(overlay_dir,'private');
  if ~mkdir(private_dir),rmdir(overlay_dir,'s');error(__mf_text__('Could not create a temporary Octave directory for publishing.', 'Yayın için geçici Octave dizini oluşturulamadı.'));endif
  fid=fopen(fullfile(overlay_dir,'publish.m'),'w');
  if fid<0,rmdir(overlay_dir,'s');error(__mf_text__('Could not copy the Octave publisher to the temporary directory.', 'Octave yayınlayıcısı geçici alana kopyalanamadı.'));endif
  complete=false;
  unwind_protect
    fputs(fid,source);
    fclose(fid);fid=-1;
    if ~copyfile(formatter_file,fullfile(private_dir,'__publish_html_output__.m'))
      error(__mf_text__('Could not copy the Octave HTML formatter to the temporary directory.', 'Octave HTML biçimlendiricisi geçici alana kopyalanamadı.'));
    endif
    helpers={'__mf_publish_figures__.m','__mf_publish_snapshot__.m','__mf_publish_equal__.m'};
    for k=1:numel(helpers)
      if ~copyfile(fullfile(fileparts(mfilename('fullpath')),helpers{k}),fullfile(private_dir,helpers{k}))
        error(__mf_text__('Could not copy the private publishing figure helper.', 'Özel yayın grafik yardımcısı kopyalanamadı.'));
      endif
    endfor
    complete=true;
  unwind_protect_cleanup
    if fid>=0,fclose(fid);endif
    if ~complete&&exist(overlay_dir,'dir')==7,rmdir(overlay_dir,'s');endif
  end_unwind_protect
endfunction

function __mf_sanitize_published_html__(output_file)
  original=fileread(output_file);
  html=__mf_publish_html__(original);
  if ~strcmp(original,html)
    % The legacy filter is defence in depth, not the security boundary. Make
    % its removals visible (the stock remote MathJax loader is one of them).
    note=__mf_text__('<p class="publish-security-note">Offline publishing: scripts and remote resource references were removed.</p>', '<p class="publish-security-note">Çevrimdışı yayın: betikler ve uzak kaynak başvuruları kaldırıldı.</p>');
    html=strrep(html,'</body>',[note '</body>']);
  endif

  temporary=[tempname(fileparts(output_file)) '.html'];
  fid=fopen(temporary,'w');
  if fid<0,error(__mf_text__('Could not safely write the published report.', 'Yayın raporu güvenli biçimde yazılamadı.'));endif
  unwind_protect
    fputs(fid,html);
    fclose(fid); fid=-1;
    [moved,message]=movefile(temporary,output_file,'f');
    if ~moved,error(__mf_text__('Could not safely write the published report: %s', 'Yayın raporu güvenli biçimde yazılamadı: %s'),message);endif
  unwind_protect_cleanup
    if fid>=0,fclose(fid);endif
    if exist(temporary,'file')==2,delete(temporary);endif
  end_unwind_protect
endfunction

% Local selector: lexical lookup survives path removal and user functions
% named __mf_text__; caller/base variables cannot enter this workspace.
function text = __mf_text__(english, turkish)
  text = english;
  if strcmp(getenv('INDYMAT_LANGUAGE'), 'tr'), text = turkish; endif
endfunction
