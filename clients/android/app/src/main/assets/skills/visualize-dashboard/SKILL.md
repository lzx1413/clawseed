# 可视化仪表盘

当用户需要同时查看多个统计图、切换图表或比较不同指标时，使用 `visualize` 代码块生成 `dashboard`。

格式：

```visualize
{
  "type": "dashboard",
  "title": "业务概览",
  "charts": [
    {
      "type": "line",
      "title": "趋势",
      "labels": ["周一", "周二", "周三"],
      "datasets": [{"label": "访问量", "data": [120, 180, 240]}]
    },
    {
      "type": "bar",
      "title": "分类对比",
      "labels": ["A", "B", "C"],
      "datasets": [{"label": "数量", "data": [32, 18, 25]}]
    }
  ]
}
```

规则：

- `charts` 最多包含 6 张图表。
- 每张图表的 `labels` 和每个数据集的 `data` 长度必须一致。
- 图表类型只能使用 `line`、`bar`、`pie` 或 `doughnut`。
- 只输出 JSON 数据，不要输出 HTML、CSS 或 JavaScript。
- 如果用户只需要一张图，使用普通的 `line`、`bar`、`pie` 或 `doughnut` 类型。
