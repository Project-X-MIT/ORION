import { useState } from "react";

import { FileUploader } from "../forms/FileUploader";
import { Form } from "../forms/Form";
import { NumberField } from "../forms/NumberField";
import { TextField } from "../forms/TextField";
import { DataTable } from "../tables/DataTable";
import type { DataTableColumn } from "../tables/DataTable";
import { Avatar } from "./Avatar";
import { Badge } from "./Badge";
import { Button } from "./Button";
import { Card } from "./Card";
import { Modal } from "./Modal";
import { Pagination } from "./Pagination";
import { Skeleton } from "./Skeleton";
import { Spinner } from "./Spinner";
import { Tabs } from "./Tabs";
import { Tooltip } from "./Tooltip";

interface ExampleRow { id: string; name: string; status: string }
const exampleRows: ExampleRow[] = [{ id: "one", name: "Example report", status: "Ready" }];
const exampleColumns: DataTableColumn<ExampleRow>[] = [
  { id: "name", header: "Name", render: (row) => row.name, sortable: true },
  { id: "status", header: "Status", render: (row) => <Badge variant="success">{row.status}</Badge> },
];

/** A living, executable catalog for shared-foundation review and composition examples. */
export function ComponentExamples() {
  const [dialogOpen, setDialogOpen] = useState(false);
  const [page, setPage] = useState(1);
  const [sort, setSort] = useState<{ columnId: string; direction: "ascending" | "descending" }>();

  return <main aria-labelledby="component-examples-title" className="ui-component-examples" id="main-content">
    <header><h1 id="component-examples-title">Shared component examples</h1><p>Canonical states and composition patterns for ORION features.</p></header>

    <section aria-labelledby="example-actions"><h2 id="example-actions">Actions and status</h2><div className="ui-component-examples__row">
      <Button>Primary action</Button><Button variant="secondary">Secondary action</Button><Button isLoading loadingLabel="Saving">Save</Button>
      <Badge variant="info">In review</Badge><Avatar alt="Ada Lovelace" status="online" />
      <Tooltip content="Additional context"><Button variant="ghost">Help</Button></Tooltip><Spinner label="Loading results" />
    </div></section>

    <section aria-labelledby="example-content"><h2 id="example-content">Content and navigation</h2>
      <Card footer={<Button variant="ghost">View details</Button>} header={<strong>Research summary</strong>}>Cards keep related content and actions together.</Card>
      <Tabs items={[{ id: "overview", label: "Overview", content: "Overview content" }, { id: "activity", label: "Activity", content: "Activity content" }]} />
      <Pagination onPageChange={setPage} page={page} pageCount={5} />
    </section>

    <section aria-labelledby="example-forms"><h2 id="example-forms">Forms</h2>
      <Form aria-label="Example form" onSubmit={(event) => event.preventDefault()} submitLabel="Save example">
        <TextField hint="Use a descriptive title." label="Title" name="title" />
        <NumberField label="Sources" min={1} name="sources" />
        <FileUploader accept=".pdf" helperText="PDF files only." label="Research files" multiple />
      </Form>
    </section>

    <section aria-labelledby="example-data"><h2 id="example-data">Data and loading</h2>
      <DataTable caption="Example reports" columns={exampleColumns} getRowId={(row) => row.id} onSortChange={setSort} rows={exampleRows} sort={sort} />
      <div aria-label="Loading preview" className="ui-component-examples__loading" role="status"><Skeleton width="70%" /><Skeleton width="45%" /></div>
    </section>

    <section aria-labelledby="example-dialog"><h2 id="example-dialog">Dialog</h2><Button onClick={() => setDialogOpen(true)}>Open example dialog</Button>
      <Modal footer={<Button onClick={() => setDialogOpen(false)}>Confirm</Button>} isOpen={dialogOpen} onClose={() => setDialogOpen(false)} title="Confirm action">Review this action before continuing.</Modal>
    </section>
  </main>;
}
