import { useState } from "react";
import "./App.css";
import FlowLogo from "./components/FlowLogo";
import ElectronInfo from "./components/ElectronInfo";

function App() {
  const [count, setCount] = useState(0);

  return (
    <div className="App">
      <header className="App-header">
        <FlowLogo />
        <h1>Flow Web</h1>
        <p>Welcome to Flow - A modern web application</p>

        <ElectronInfo />

        <div className="card">
          <button onClick={() => setCount((count) => count + 1)}>
            count is {count}
          </button>
          <p>
            Edit <code>src/App.jsx</code> and save to test HMR
          </p>
        </div>

        <p className="read-the-docs">Click on the Flow logo to learn more</p>
      </header>
    </div>
  );
}

export default App;
