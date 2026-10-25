// The screens of later stages (B: Wallet; F4: My tickets, Backup; F5: Rounds; F6: Vault, How it works):
// their title only, until they are built.
export function Placeholder({ title, testId }: { title: string; testId: string }) {
  return (
    <section
      data-testid={testId}
      aria-labelledby={`${testId}-title`}
      className="rounded-[14px] border border-rule bg-surface p-4"
    >
      <h1 id={`${testId}-title`} className="m-0 text-[17px] font-semibold">
        {title}
      </h1>
    </section>
  );
}
