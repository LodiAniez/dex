import type { Block, Inline } from "./markdown";

/**
 * Renders parsed markdown as React elements. There is deliberately no
 * `dangerouslySetInnerHTML` anywhere in this file: the parser produces a tree,
 * and this turns the tree into elements, so file content can never become
 * markup. Links do not navigate — this is a webview, and a click on a bad
 * href would replace the whole app — they show their target on hover.
 */
export function Markdown({ blocks }: { blocks: Block[] }) {
  return (
    <div className="md">
      {blocks.map((block, i) => (
        <BlockView key={i} block={block} />
      ))}
    </div>
  );
}

function BlockView({ block }: { block: Block }) {
  switch (block.kind) {
    case "heading": {
      const Tag = `h${Math.min(block.level, 6)}` as "h1" | "h2" | "h3" | "h4" | "h5" | "h6";
      return (
        <Tag>
          <Inlines nodes={block.children} />
        </Tag>
      );
    }
    case "paragraph":
      return (
        <p>
          <Inlines nodes={block.children} />
        </p>
      );
    case "code":
      return (
        <pre className="md-code" data-lang={block.lang ?? undefined}>
          <code>{block.text}</code>
        </pre>
      );
    case "quote":
      return (
        <blockquote>
          <Inlines nodes={block.children} />
        </blockquote>
      );
    case "rule":
      return <hr />;
    case "list": {
      const Tag = block.ordered ? "ol" : "ul";
      return (
        <Tag>
          {block.items.map((item, i) => (
            <li key={i} style={{ marginLeft: `${item.depth * 1.25}em` }} className={item.checked === null ? undefined : "md-task"}>
              {item.checked !== null && <input type="checkbox" checked={item.checked} readOnly tabIndex={-1} />}
              <Inlines nodes={item.children} />
            </li>
          ))}
        </Tag>
      );
    }
    case "table":
      return (
        <table>
          <thead>
            <tr>
              {block.header.map((cell, i) => (
                <th key={i}>
                  <Inlines nodes={cell} />
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {block.rows.map((row, r) => (
              <tr key={r}>
                {row.map((cell, c) => (
                  <td key={c}>
                    <Inlines nodes={cell} />
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      );
  }
}

function Inlines({ nodes }: { nodes: Inline[] }) {
  return (
    <>
      {nodes.map((node, i) => {
        switch (node.kind) {
          case "text":
            return <span key={i}>{node.text}</span>;
          case "code":
            return <code key={i}>{node.text}</code>;
          case "strong":
            return (
              <strong key={i}>
                <Inlines nodes={node.children} />
              </strong>
            );
          case "em":
            return (
              <em key={i}>
                <Inlines nodes={node.children} />
              </em>
            );
          case "link":
            return (
              <span key={i} className="md-link" title={node.href}>
                <Inlines nodes={node.children} />
              </span>
            );
        }
      })}
    </>
  );
}
