function clean = __mf_publish_html__(html)
% Sanitize generated publish markup, preserving text and source comments.
% This is an offline resource filter, not a sandbox for arbitrary user HTML.
  parts={}; pos=1; count=length(html);
  while pos<=count
    offset=find(html(pos:end)=='<',1);
    if isempty(offset),parts{end+1}=html(pos:end);break;endif
    first=pos+offset-1;
    parts{end+1}=html(pos:first-1);
    if strncmp(html(first:end),'<!--',4)
      tail=strfind(html(first+4:end),'-->');
      if isempty(tail),last=count;else,last=first+tail(1)+5;endif
      parts{end+1}=html(first:last);pos=last+1;continue;
    endif
    [~,name_end,~,~,token]=regexp(html(first:end),'^<(/?)([A-Za-z][A-Za-z0-9:-]*)(?=[\s/>])','once');
    if isempty(name_end),parts{end+1}='<';pos=first+1;continue;endif
    last=__mf_html_tag_end__(html,first+name_end);
    tag=html(first:last);name=lower(token{2});closing=~isempty(token{1});
    pos=last+1;
    if ~closing && any(strcmp(name,{'script','style','textarea','title'}))
      [close_start,close_end]=regexp(html(pos:end),['</' name '\s*>'],'once','ignorecase');
      if isempty(close_start),body=html(pos:end);ending='';pos=count+1;
      else
        body=html(pos:pos+close_start-2);ending=html(pos+close_start-1:pos+close_end-1);pos=pos+close_end;
      endif
      % Keep lane M's inert fallback: no scripts, including inline MathJax.
      if strcmp(name,'script'),continue;endif
      if strcmp(name,'style'),body=__mf_publish_css__(body);endif
      parts{end+1}=[__mf_html_attributes__(tag,name,name_end) body ending];
    elseif closing
      parts{end+1}=tag;
    else
      parts{end+1}=__mf_html_attributes__(tag,name,name_end);
    endif
  endwhile
  clean=[parts{:}];
endfunction

function last = __mf_html_tag_end__(html,pos)
  quote=char(0);last=length(html);
  for k=pos:length(html)
    c=html(k);
    if quote~=char(0)
      if c==quote,quote=char(0);endif
    elseif c=='"' || c==''''
      quote=c;
    elseif c=='>'
      last=k;return;
    endif
  endfor
endfunction

function clean = __mf_html_attributes__(tag,name,name_end)
  resource={};
  switch name
    case 'link',resource={'href'};
    case {'img','source'},resource={'src','srcset'};
    case {'iframe','embed','audio','track','input'},resource={'src'};
    case 'object',resource={'data','codebase'};
    case 'video',resource={'src','poster'};
  endswitch
  parts={};copied=1;pos=name_end+1;count=length(tag);
  while pos<count
    if isspace(tag(pos)) || tag(pos)=='/',pos+=1;continue;endif
    first=pos;
    while pos<count && ~isspace(tag(pos)) && ~any(tag(pos)=='=/>'),pos+=1;endwhile
    if pos==first,pos+=1;continue;endif
    attribute=lower(tag(first:pos-1));
    while pos<count && isspace(tag(pos)),pos+=1;endwhile
    if pos>=count || tag(pos)~='=',continue;endif
    pos+=1;
    while pos<count && isspace(tag(pos)),pos+=1;endwhile
    if pos>=count,break;endif
    quote=char(0);
    if tag(pos)=='"' || tag(pos)=='''',quote=tag(pos);pos+=1;endif
    value_start=pos;
    if quote~=char(0)
      while pos<count && tag(pos)~=quote,pos+=1;endwhile
    else
      while pos<count && ~isspace(tag(pos)),pos+=1;endwhile
    endif
    value_end=pos-1;value=tag(value_start:value_end);
    if quote~=char(0) && pos<count,pos+=1;endif
    if any(strcmp(attribute,resource)) && (__mf_remote_url__(value) || ...
        (strcmp(attribute,'srcset') && ~isempty(regexpi(value,'(?:^|[,\s])(?:https?:)?//','once'))))
      parts{end+1}=tag(copied:first-1);copied=pos;
    elseif strcmp(attribute,'style')
      parts{end+1}=[tag(copied:value_start-1) __mf_publish_css__(value)];copied=value_end+1;
    endif
  endwhile
  parts{end+1}=tag(copied:end);clean=[parts{:}];
endfunction

function yes = __mf_remote_url__(value)
  yes=~isempty(regexpi(strtrim(value),'^(?:https?:)?//','once'));
endfunction

function clean = __mf_publish_css__(css)
% Only called for actual CSS, never for code listings or program output.
  parts={};pos=1;copied=1;count=length(css);
  while pos<=count
    if strncmp(css(pos:end),'/*',2)
      tail=strfind(css(pos+2:end),'*/');
      if isempty(tail),break;endif
      pos+=tail(1)+3;continue;
    endif
    if css(pos)=='"' || css(pos)==''''
      pos=__mf_css_quote_end__(css,pos)+1;continue;
    endif
    import_end=regexp(css(pos:end),'^@import\s+','end','once','ignorecase');
    if ~isempty(import_end)
      target=pos+import_end;
      [url_end,value]=__mf_css_url__(css,target);
      if url_end==0 && target<=count && any(css(target)==['"' ''''])
        url_end=__mf_css_quote_end__(css,target);value=css(target+1:url_end-1);
      endif
      if url_end>0 && __mf_remote_url__(value)
        last=url_end+1;
        while last<=count
          if any(css(last)==['"' '''']),last=__mf_css_quote_end__(css,last);
          elseif css(last)==';',break;endif
          last+=1;
        endwhile
        parts{end+1}=css(copied:pos-1);pos=min(last+1,count+1);copied=pos;continue;
      endif
    endif
    [last,value]=__mf_css_url__(css,pos);
    if last>0
      if __mf_remote_url__(value)
        parts{end+1}=[css(copied:pos-1) 'url()'];copied=last+1;
      endif
      pos=last+1;
    else
      pos+=1;
    endif
  endwhile
  parts{end+1}=css(copied:end);clean=[parts{:}];
endfunction

function [last,value] = __mf_css_url__(css,pos)
  last=0;value='';
  if pos>1 && ~isempty(regexp(css(pos-1),'[A-Za-z0-9_-]','once')),return;endif
  prefix=regexp(css(pos:end),'^url\s*\(\s*','end','once','ignorecase');
  if isempty(prefix),return;endif
  first=pos+prefix;k=first;
  if k<=length(css) && any(css(k)==['"' ''''])
    k=__mf_css_quote_end__(css,k);value=css(first+1:k-1);k+=1;
    while k<=length(css) && isspace(css(k)),k+=1;endwhile
  else
    while k<=length(css) && css(k)~=')',k+=1;endwhile
    value=strtrim(css(first:k-1));
  endif
  if k<=length(css) && css(k)==')',last=k;endif
endfunction

function last = __mf_css_quote_end__(css,first)
  last=first+1;
  while last<=length(css)
    if css(last)==char(92),last+=2;continue;endif
    if css(last)==css(first),return;endif
    last+=1;
  endwhile
  last=length(css);
endfunction
