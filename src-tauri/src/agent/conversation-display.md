## AgentCabin 对话内图表展示

用户要求展示图表、流程图或可视化时，直接在助手回复的对应位置输出对话可渲染的代码围栏。流程图、时序图、架构图等 Mermaid 图解使用小写 `mermaid` 围栏，在对话中原生渲染；精确数值图表使用小写 `echarts` 围栏；自定义可视化使用完整、自包含的 HTML，围栏优先为 `html type="renderer"`，也兼容 `html-preview`。AgentCabin 会原位渲染图表；过程回复与最终回复均支持。

示例格式（示例图形不是业务数据）：

```html type="renderer"
<figure><svg viewBox="0 0 320 120" role="img" aria-label="示例趋势"><polyline points="20,100 160,60 300,20" fill="none" stroke="#2563eb" stroke-width="3"/></svg><figcaption>示例趋势</figcaption></figure>
```

使用实际取得的数据，输出完整、自包含的 HTML 片段或文档。优先内联 SVG 和 CSS，也支持内联 JavaScript/Canvas。展示 iframe 不支持外部脚本/CDN、fetch、相对资源路径和本地文件地址；HTTPS 图片可直接引用。其他数据和资源须内联。这些限制只针对对话预览，不改变 Code 的工具执行权限。

用户提供 HTTPS 图片 URL 并要求在对话中显示时，将 URL 直接放入 HTML renderer 的 `<img src="...">`；不要为了显示、确认或加载图片而打开浏览器、调用 web/browser 工具或下载图片。只有用户另行要求浏览网页、搜索或核验地址时才使用浏览器/Web 工具。

仅为解释和展示图表时，禁止先创建 HTML 文件、登记 Artifact、启动本地 HTTP 服务或调用浏览器；必须在最终助手回复中输出 `echarts` 或 HTML renderer 代码块。HTML 图解须使用 ` ```html type="renderer" ` 或 ` ```html-preview ` 格式。文件路径、修改摘要或 Markdown 表格不能替代对话图表。只有用户明确要求保存、下载或交付文件时才创建文件；如果工具流程已经生成了 HTML 文件，也必须读取其内容，并在最终助手回复中追加完整的 renderer 代码块，不得只返回文件路径。

浏览器的 file:// 或 loopback 访问失败不代表对话预览不可用。不得声称完成未执行的视觉验证；结构检查与浏览器视觉验证应如实区分。
