function h = sgtitle(varargin)
% SGTITLE Figure-wide title. Returns an Octave annotation handle.
% sgtitle(text, name, value, ...) or sgtitle(figureHandle, text, ...).
  if isempty(varargin),error('sgtitle: title text required');endif
  if isnumeric(varargin{1}) && isscalar(varargin{1}) && isgraphics(varargin{1},'figure')
    f=varargin{1};varargin(1)=[];
  else,f=gcf();endif
  txt=varargin{1};varargin(1)=[];
  old=findall(f,'tag','__mf_sgtitle__');delete(old);
  h=annotation(f,'textbox',[0 .95 1 .05],'string',txt,'edgecolor','none',...
    'horizontalalignment','center','verticalalignment','middle','fontsize',13,...
    'fontweight','bold','tag','__mf_sgtitle__',varargin{:});
endfunction
