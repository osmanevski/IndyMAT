function [triangles, owners, edges, code, args] = __mf_patch3d_geometry__(vertices, faces)
  % Pure bounded simple planar polygons; one-based numeric source connectivity.
  % NaN is trailing face padding only. A repeated closing index is removed.
  % EPS=1e-10 in face-local normalised coordinates; ambiguous cases fall back.
  % First valid ear is scanned in remaining source order after each removal.
  triangles = zeros(0,3); owners = zeros(0,1); edges = zeros(0,3);
  code = ''; args = struct();
  if ~isnumeric(vertices) || ~isreal(vertices) || issparse(vertices) || ndims(vertices) ~= 2 || size(vertices,2) ~= 3
    code = 'invalid_data'; args = struct('property','vertices'); return;
  end
  if ~isnumeric(faces) || ~isreal(faces) || issparse(faces) || ndims(faces) ~= 2
    code = 'invalid_data'; args = struct('property','faces'); return;
  end
  if size(vertices,1) > 40000
    code = 'budget_exceeded'; args = struct('budget','patch_vertices','actual',size(vertices,1),'limit',40000); return;
  end
  if size(faces,1) > 80000
    code = 'budget_exceeded'; args = struct('budget','patch_triangles','actual',size(faces,1),'limit',80000); return;
  end
  % Bound the input width before row copies; 65 allows a closing index.
  if size(faces,2) > 65
    code = 'budget_exceeded'; args = struct('budget','polygon_vertices','actual',size(faces,2),'limit',64); return;
  end
  count = 0; polygons = cell(size(faces,1),1);
  for f = 1:size(faces,1)
    row = faces(f,:); padding = find(isnan(row),1);
    if ~isempty(padding)
      if any(~isnan(row(padding:end)))
        code = 'invalid_data'; args = struct('property','faces'); return;
      end
      row = row(1:padding-1);
    end
    if numel(row) > 3 && row(1) == row(end), row = row(1:end-1); end
    if numel(row) < 3
      code = 'invalid_data'; args = struct('property','faces'); return;
    end
    if numel(row) > 64
      code = 'budget_exceeded'; args = struct('budget','polygon_vertices','actual',numel(row),'limit',64); return;
    end
    count = count + numel(row)-2;
    if count > 80000
      code = 'budget_exceeded'; args = struct('budget','patch_triangles','actual',count,'limit',80000); return;
    end
    if any(~isfinite(row) | row ~= fix(row) | row < 1 | row > size(vertices,1))
      code = 'invalid_index'; args = struct('property','faces'); return;
    end
    polygons{f} = double(row);
  end
  if any(~isfinite(vertices(:)))
    code = 'invalid_data'; args = struct('property','vertices'); return;
  end
  vertices = double(vertices);
  % Allocate once after bounds are checked; no partial geometry on rejection.
  triangles = zeros(count,3); owners = zeros(count,1);
  edge_count = count + 2*numel(polygons); edges = zeros(edge_count,3);
  ti = 0; ei = 0;
  for f = 1:numel(polygons)
    face = polygons{f}; local = mf_polygon(vertices,face);
    if isempty(local)
      triangles = zeros(0,3); owners = zeros(0,1); edges = zeros(0,3);
      code = 'unsupported_patch'; args = struct('property','polygon','face',f,'detail','invalid_geometry'); return;
    end
    rows = ti+(1:size(local,1)); triangles(rows,:) = local; owners(rows) = f;
    ti = ti + size(local,1);
    rows = ei+(1:numel(face)); edges(rows,:) = [face(:),face([2:end,1])',repmat(f,numel(face),1)];
    ei = ei + numel(face);
  end
end

function triangles = mf_polygon(vertices,face)
  EPS = 1e-10; triangles = zeros(0,3);
  points = vertices(face,:); scale = max(abs(points(:)));
  if ~(scale > 0), return; end
  points = points/scale; points = points - points(1,:);
  extent = max(abs(points(:))); if ~(extent > 0), return; end
  points = points/extent; n = size(points,1);
  for i = 1:n
    for j = i+1:n
      if max(abs(points(i,:)-points(j,:))) <= EPS, return; end
    end
  end
  normal = [0,0,0];
  for i = 1:n
    a = points(i,:); b = points(mod(i,n)+1,:);
    normal(1) = normal(1)+(a(2)-b(2))*(a(3)+b(3));
    normal(2) = normal(2)+(a(3)-b(3))*(a(1)+b(1));
    normal(3) = normal(3)+(a(1)-b(1))*(a(2)+b(2));
  end
  len = sqrt(sum(normal.*normal));
  if ~(len > EPS), return; end
  for i = 1:n
    if abs(sum(points(i,:).*normal)) > EPS*len, return; end
  end
  [~,drop] = max(abs(normal)); xy = points(:,setdiff(1:3,drop));
  for i = 1:n
    previous = xy(mod(i-2,n)+1,:); next = xy(mod(i,n)+1,:);
    if abs(mf_cross(previous,xy(i,:),next)) <= EPS && sum((previous-xy(i,:)).*(next-xy(i,:))) > EPS, return; end
    for j = i+1:n
      if j == i+1 || (i == 1 && j == n), continue; end
      if mf_intersects(xy(i,:),xy(mod(i,n)+1,:),xy(j,:),xy(mod(j,n)+1,:),EPS), return; end
    end
  end
  area = 0;
  for i = 1:n
    j = mod(i,n)+1; area = area+xy(i,1)*xy(j,2)-xy(j,1)*xy(i,2);
  end
  if abs(area) <= EPS, return; end
  winding = sign(area); remaining = 1:n; result = zeros(n-2,3); ti = 0;
  while numel(remaining) > 3
    ear = 0; m = numel(remaining);
    for i = 1:m
      a = remaining(mod(i-2,m)+1); b = remaining(i); c = remaining(mod(i,m)+1);
      if winding*mf_cross(xy(a,:),xy(b,:),xy(c,:)) <= EPS, continue; end
      blocked = false;
      for p = remaining
        if p == a || p == b || p == c, continue; end
        if winding*mf_cross(xy(a,:),xy(b,:),xy(p,:)) >= -EPS && winding*mf_cross(xy(b,:),xy(c,:),xy(p,:)) >= -EPS && winding*mf_cross(xy(c,:),xy(a,:),xy(p,:)) >= -EPS
          blocked = true; break;
        end
      end
      if blocked, continue; end
      ear = i; ti = ti+1; result(ti,:) = face([a,b,c]); break;
    end
    if ear == 0, return; end
    remaining(ear) = [];
  end
  if winding*mf_cross(xy(remaining(1),:),xy(remaining(2),:),xy(remaining(3),:)) <= EPS, return; end
  result(ti+1,:) = face(remaining); triangles = result;
end

function value = mf_cross(a,b,c)
  value = (b(1)-a(1))*(c(2)-a(2))-(b(2)-a(2))*(c(1)-a(1));
end
function yes = mf_on(a,b,p,EPS)
  yes = abs(mf_cross(a,b,p)) <= EPS && all(p >= min(a,b)-EPS & p <= max(a,b)+EPS);
end
function yes = mf_intersects(a,b,c,d,EPS)
  ac = mf_cross(a,b,c); ad = mf_cross(a,b,d); ca = mf_cross(c,d,a); cb = mf_cross(c,d,b);
  yes = (((ac > EPS && ad < -EPS) || (ac < -EPS && ad > EPS)) && ((ca > EPS && cb < -EPS) || (ca < -EPS && cb > EPS))) || mf_on(a,b,c,EPS) || mf_on(a,b,d,EPS) || mf_on(c,d,a,EPS) || mf_on(c,d,b,EPS);
end
