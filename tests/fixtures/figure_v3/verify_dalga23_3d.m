% Manual browser acceptance examples. Run only in a fresh verification copy.
% This lane did not execute these graphics commands.
figure('name','D23 hidden rulers');
plot3([0 1 2],[0 1 0],[0 1 2],'-o','linewidth',3,...
      'markersize',12,'color',[.8 .2 .4],'markerfacecolor',[.8 .2 .4]);
title('Visible title'); xlabel('Visible X'); ylabel('Visible Y'); zlabel('Visible Z');
legend('Visible track'); grid on; axis off;

for d23_orientation = 1:2
  for d23_reverse = 0:1
    figure('name',sprintf('D23 extreme bar %d reverse %d',d23_orientation,d23_reverse));
    surf([0 1 2],[0 1],[0 1 0;1 0 1],'edgecolor','none');
    colormap([1 0 0;0 1 0;0 0 1]); caxis([-1e308 1e308]);
    if d23_orientation == 1
      d23_bar = colorbar('eastoutside'); d23_key = 'y';
    else
      d23_bar = colorbar('southoutside'); d23_key = 'x';
    end
    set(d23_bar,[d23_key 'tick'],[-1e308 0 1e308],...
        [d23_key 'ticklabel'],{'low','mid','high'});
    if d23_reverse, set(d23_bar,[d23_key 'dir'],'reverse'); end
  end
end

figure('name','D23 200 mesh');
[d23_X,d23_Y] = meshgrid(linspace(-2,2,200));
d23_Z = sin(d23_X).*cos(d23_Y);
mesh(d23_X,d23_Y,d23_Z); title('200 by 200 grid'); colorbar; grid on;

figure('name','D23 marker sizes and gaps');
plot3([0 1 NaN 2 3],[0 1 NaN 0 1],[0 1 NaN 1 0],'--o','linewidth',2);
hold on;
scatter3([0 1 2],[.5 .5 .5],[.5 .5 .5],[0 25 100],[.8 .2 .4],'filled');
hold off; title('Gap and zero/variable marker sizes'); grid on;

figure('name','D23 oversized fallback');
surf(zeros(500,500)); title('500 by 500 PNG fallback');
